#include "WhistleblowerBackend.h"

#include <QCoreApplication>
#include <QDateTime>
#include <QFileInfo>
#include <QFuture>
#include <QFutureWatcher>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMimeDatabase>
#include <QMimeType>
#include <QThreadPool>
#include <QtConcurrent/QtConcurrent>

// C FFI from ui/ffi/ — resolved at runtime via dlopen / co-located dylib.
extern "C" {
    char* whistleblower_anchor_one(const char* args_json);
    char* whistleblower_query_by_cid(const char* args_json);
    char* whistleblower_compute_metadata_hash(const char* args_json);
    char* whistleblower_version();
    void  whistleblower_free_string(char* s);
}

static QString callFfiRaw(char* (*fn)(const char*), const QJsonObject& args) {
    QByteArray json = QJsonDocument(args).toJson(QJsonDocument::Compact);
    char* raw = fn(json.constData());
    if (!raw) return R"({"success":false,"error":"null return from FFI"})";
    QString result = QString::fromUtf8(raw);
    whistleblower_free_string(raw);
    return result;
}

WhistleblowerBackend::WhistleblowerBackend(LogosAPI* api, QObject* parent)
    : QObject(parent)
    , m_api(api)
    , m_walletPath(qEnvironmentVariable("NSSA_WALLET_HOME_DIR", ".scaffold/wallet"))
    , m_sequencerUrl(qEnvironmentVariable("NSSA_SEQUENCER_URL", "http://127.0.0.1:3040"))
{}

WhistleblowerBackend::~WhistleblowerBackend() = default;

QJsonObject WhistleblowerBackend::baseFfiArgs() const {
    return QJsonObject{
        {"wallet_path",   m_walletPath},
        {"sequencer_url", m_sequencerUrl},
    };
}

void WhistleblowerBackend::setBusy(bool busy, const QString& message) {
    if (m_busy == busy && m_busyMessage == message) return;
    m_busy = busy;
    m_busyMessage = message;
    emit busyChanged();
}

void WhistleblowerBackend::setStage(int s) {
    if (m_stage == s) return;
    m_stage = s;
    emit stageChanged();
}

void WhistleblowerBackend::setError(const QString& stage, const QString& msg) {
    m_lastError = msg;
    emit lastErrorChanged();
    emit error(stage, msg);
    setBusy(false, "");
}

void WhistleblowerBackend::setSelectedFile(const QString& filePath) {
    QString normalized = filePath;
    if (normalized.startsWith("file://")) normalized.remove(0, 7);
    if (m_selectedFile == normalized) return;
    m_selectedFile = normalized;
    emit selectedFileChanged();
    setStage(0);
}

void WhistleblowerBackend::publish(
    const QString& title,
    const QString& description,
    const QString& tagsCsv)
{
    if (m_selectedFile.isEmpty()) {
        setError("publish", "no file selected");
        return;
    }
    QFileInfo fi(m_selectedFile);
    if (!fi.exists() || !fi.isReadable()) {
        setError("publish", "file does not exist or is not readable: " + m_selectedFile);
        return;
    }
    qint64 sizeBytes = fi.size();
    QString contentType = QMimeDatabase().mimeTypeForFile(fi).name();
    QStringList tags;
    for (const QString& tag : tagsCsv.split(',', Qt::SkipEmptyParts)) {
        tags << tag.trimmed();
    }

    setBusy(true, "uploading to Logos Storage…");
    setStage(1);
    m_lastError.clear();
    emit lastErrorChanged();

    uploadToStorage(m_selectedFile, [this, title, description, contentType, sizeBytes, tags](QString cid) {
        if (cid.isEmpty()) return; // setError already invoked

        m_lastCid = cid;
        emit lastCidChanged();
        emit uploadComplete(cid);
        setBusy(true, "computing metadata hash…");

        // Build the canonical envelope + hash via the Rust FFI.
        QString metadataHashHex;
        QByteArray envelopeBytes;
        if (!computeEnvelope(cid, title, description, contentType, sizeBytes, tags,
                             &metadataHashHex, &envelopeBytes))
        {
            return; // setError already invoked
        }
        m_lastMetadataHashHex = metadataHashHex;

        setBusy(true, "broadcasting to Logos Delivery…");
        setStage(2);
        const QString topic = "/lp0017-whistleblower/1/cids/json";
        broadcastEnvelope(topic, envelopeBytes, [this](QString messageHash) {
            if (messageHash.isEmpty()) return;
            emit broadcastComplete(messageHash);
            setStage(2); // ready to anchor
            setBusy(false, "");
        });
    });
}

void WhistleblowerBackend::anchorLast() {
    if (m_lastCid.isEmpty() || m_lastMetadataHashHex.isEmpty()) {
        setError("anchor", "no published document to anchor");
        return;
    }
    const QString cid = m_lastCid;
    const QString metadataHashHex = m_lastMetadataHashHex;

    setBusy(true, "anchoring on LEZ…");

    auto* watcher = new QFutureWatcher<QJsonObject>(this);
    connect(watcher, &QFutureWatcher<QJsonObject>::finished, this,
        [this, watcher, cid]() {
            QJsonObject obj = watcher->result();
            watcher->deleteLater();
            if (!obj.value("success").toBool()) {
                setError("anchor", obj.value("error").toString("unknown"));
                return;
            }
            QJsonObject entry = obj.value("entry").toObject();
            m_lastAnchorTimestamp = static_cast<qint64>(entry.value("anchor_timestamp").toDouble());
            emit lastAnchorTimestampChanged();
            QString cidHash = entry.value("cid_hash").toString();
            emit anchorComplete(cidHash);
            setStage(3);
            setBusy(false, "");
        });
    watcher->setFuture(QtConcurrent::run([this, cid, metadataHashHex]() {
        return anchorOneFfi(cid, metadataHashHex);
    }));
}

QJsonObject WhistleblowerBackend::anchorOneFfi(const QString& cid, const QString& metadataHashHex) {
    QJsonObject args = baseFfiArgs();
    args["cid"] = cid;
    args["metadata_hash_hex"] = metadataHashHex;
    QString result = callFfiRaw(whistleblower_anchor_one, args);
    return QJsonDocument::fromJson(result.toUtf8()).object();
}

bool WhistleblowerBackend::computeEnvelope(
    const QString& cid,
    const QString& title,
    const QString& description,
    const QString& contentType,
    qint64 sizeBytes,
    const QStringList& tags,
    QString* outMetadataHashHex,
    QByteArray* outEnvelopeBytes)
{
    QJsonArray tagsJson;
    for (const QString& t : tags) tagsJson.append(t);

    QJsonObject args{
        {"cid", cid},
        {"title", title},
        {"description", description},
        {"content_type", contentType},
        {"size_bytes", static_cast<double>(sizeBytes)},
        {"timestamp_unix", static_cast<double>(QDateTime::currentSecsSinceEpoch())},
        {"tags", tagsJson},
    };
    QString result = callFfiRaw(whistleblower_compute_metadata_hash, args);
    QJsonObject obj = QJsonDocument::fromJson(result.toUtf8()).object();
    if (!obj.value("success").toBool()) {
        setError("compute envelope", obj.value("error").toString("unknown"));
        return false;
    }
    *outMetadataHashHex = obj.value("metadata_hash_hex").toString();
    *outEnvelopeBytes = QByteArray::fromBase64(
        obj.value("envelope_bytes_b64").toString().toUtf8());
    return true;
}

// ─── Storage / Delivery integration via LogosAPI (TODO when API surface confirmed) ───
//
// Both methods below call into the Logos Core modules that Basecamp has
// already loaded into this process. The exact LogosAPI invocation pattern
// is TBD pending a worked example — whisper-wall stores `LogosAPI* m_api`
// but doesn't actually use it (whisper-wall is on-chain-only). We need to
// confirm one of these patterns:
//
//   a) m_api->getModule("storage_module") returns a QObject* on which we
//      can use QMetaObject::invokeMethod for Q_INVOKABLE methods + connect
//      to its signals.
//   b) m_api exposes a higher-level wrapper (e.g. m_api->storage()) with
//      typed methods.
//   c) The modules are accessed through QtRemoteObjects across processes,
//      requiring a QRemoteObjectNode.
//
// The headers in SPECS/refs/ confirm the storage/delivery modules expose:
//   storage_module: uploadUrl(QUrl, int chunkSize) -> LogosResult
//                   signal storageUploadDone(QString sessionId, QString cid)
//   delivery_module: send(QString topic, QByteArray payload) -> LogosResult
//                    signal messageSent(QString messageId, QString messageHash)
//
// For a non-blocking demo the C++ side calls the Q_INVOKABLE method, sets
// up a QObject::connect to the corresponding "done" signal with a one-shot
// lambda, and the lambda invokes our onComplete callback with the CID /
// message hash extracted from the signal.

void WhistleblowerBackend::uploadToStorage(
    const QString& filePath,
    std::function<void(QString)> onComplete)
{
    // TODO(Phase-1.7-runtime): replace this stub with a real call that
    // invokes storage_module.uploadUrl(QUrl::fromLocalFile(filePath), 65536)
    // and connects to its storageUploadDone signal. The lambda below
    // simulates the success path so the rest of the pipeline can be
    // exercised against a mock for now.
    Q_UNUSED(filePath)
    if (!m_api) {
        setError("upload", "no LogosAPI handle — running outside Basecamp host");
        onComplete(QString());
        return;
    }
    setError("upload", "storage_module integration not yet wired — see WhistleblowerBackend.cpp TODO");
    onComplete(QString());
}

void WhistleblowerBackend::broadcastEnvelope(
    const QString& topic,
    const QByteArray& envelopeBytes,
    std::function<void(QString)> onComplete)
{
    // TODO(Phase-1.7-runtime): same shape as uploadToStorage — invoke
    // delivery_module.send(topic, envelopeBytes), connect messageSent.
    Q_UNUSED(topic)
    Q_UNUSED(envelopeBytes)
    if (!m_api) {
        setError("broadcast", "no LogosAPI handle");
        onComplete(QString());
        return;
    }
    setError("broadcast", "delivery_module integration not yet wired");
    onComplete(QString());
}
