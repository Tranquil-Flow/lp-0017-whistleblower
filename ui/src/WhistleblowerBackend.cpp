#include "WhistleblowerBackend.h"

#include <QCoreApplication>
#include <QDateTime>
#include <QDebug>
#include <QFile>
#include <QFileInfo>
#include <QFuture>
#include <QFutureWatcher>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMimeDatabase>
#include <QMimeType>
#include <QThreadPool>
#include <QUrl>
#include <QVariant>
#include <QVariantList>
#include <QtConcurrent/QtConcurrent>

#include "logos_api.h"
#include "logos_api_client.h"
#include "logos_object.h"
#include "logos_mode.h"

namespace {
// Read a JSON config from a Qt resource (`qrc:/configs/<name>`). Returns the
// raw string contents — storage_module / delivery_module both expect the
// caller to pass the full JSON document, not a path. Returns empty on failure
// (caller logs + skips init).
QString readBundledConfig(const QString& name) {
    QFile f(QStringLiteral(":/configs/") + name);
    if (!f.open(QIODevice::ReadOnly)) {
        qWarning() << "WhistleblowerBackend: could not open bundled config:" << name;
        return {};
    }
    return QString::fromUtf8(f.readAll());
}
}

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
{
    if (m_api) {
        // Resolve LogosAPIClient handles + LogosObject* references for the two
        // modules we depend on. Both modules are declared in manifest.json so
        // Basecamp guarantees they're loaded before we are constructed.
        m_storageClient = m_api->getClient("storage_module");
        m_deliveryClient = m_api->getClient("delivery_module");
        if (m_storageClient) {
            m_storageObject = m_storageClient->requestObject("storage_module");
            // storage_module needs init(config) + start() before any upload.
            // The bundled config (ui/configs/storage_config.json) ships real
            // SPRs (Signed Peer Records) for the public Logos storage
            // network, sourced from logos-co/node-configs#feat/storage-config-2.
            // Failure here doesn't crash the plugin — uploads will just hit
            // their 60s safety timeout if the module never gets ready.
            const QString storageCfg = readBundledConfig("storage_config.json");
            if (!storageCfg.isEmpty()) {
                // Use the QVariantList overload explicitly to avoid C++ picking
                // the (QVariant, QVariant, timeout) overload — int 30000 would
                // get coerced into a second positional arg otherwise.
                qInfo() << "WhistleblowerBackend: storage_module.init() …";
                QVariantList initArgs{QVariant(storageCfg)};
                QVariant initOk = m_storageClient->invokeRemoteMethod(
                    "storage_module", "init", initArgs, Timeout(30000));
                qInfo() << "WhistleblowerBackend: storage_module.init() ->" << initOk;
                qInfo() << "WhistleblowerBackend: storage_module.start() …";
                QVariantList startArgs;
                QVariant startOk = m_storageClient->invokeRemoteMethod(
                    "storage_module", "start", startArgs, Timeout(30000));
                qInfo() << "WhistleblowerBackend: storage_module.start() ->" << startOk;
            }
            // Subscribe to the upload-completion event up front. The lambda
            // captures `this` and dispatches to whichever pending upload
            // callback is currently active (m_pendingUploadCallback).
            if (m_storageObject) {
                m_storageClient->onEvent(m_storageObject, "storageUploadDone",
                    [this](const QString&, const QVariantList& data) {
                        // storage_module emits the CID as the last QString in
                        // the data list. Be defensive about position — log says
                        // session id may precede it.
                        QString cid;
                        for (const QVariant& v : data) {
                            QString s = v.toString();
                            if (s.startsWith("z") || s.startsWith("bafy")) {
                                cid = s;
                                break;
                            }
                        }
                        if (cid.isEmpty() && !data.isEmpty()) {
                            cid = data.last().toString();
                        }
                        if (m_pendingUploadCallback) {
                            auto cb = m_pendingUploadCallback;
                            m_pendingUploadCallback = nullptr;
                            cb(cid);
                        }
                    });
            }
        }
        if (m_deliveryClient) {
            m_deliveryObject = m_deliveryClient->requestObject("delivery_module");
            // delivery_module needs init(config) + start() before send().
            // Bundled config ships {"mode":"Core","preset":"logos.dev"} —
            // the preset key resolves to liblogosdelivery's compiled-in
            // bootstrap peer list for the public logos.dev network (see
            // github.com/logos-messaging/logos-delivery
            // waku/factory/networks_config.nim::LogosDevConf).
            const QString deliveryCfg = readBundledConfig("delivery_config.json");
            if (!deliveryCfg.isEmpty()) {
                qInfo() << "WhistleblowerBackend: delivery_module.init() …";
                QVariantList initArgs{QVariant(deliveryCfg)};
                QVariant initOk = m_deliveryClient->invokeRemoteMethod(
                    "delivery_module", "init", initArgs, Timeout(30000));
                qInfo() << "WhistleblowerBackend: delivery_module.init() ->" << initOk;
                qInfo() << "WhistleblowerBackend: delivery_module.start() …";
                QVariantList startArgs;
                QVariant startOk = m_deliveryClient->invokeRemoteMethod(
                    "delivery_module", "start", startArgs, Timeout(30000));
                qInfo() << "WhistleblowerBackend: delivery_module.start() ->" << startOk;
            }
            if (m_deliveryObject) {
                // Per delivery_module_plugin.h: data[0]=request id, data[1]=hash,
                // data[2]=timestamp.
                m_deliveryClient->onEvent(m_deliveryObject, "messageSent",
                    [this](const QString&, const QVariantList& data) {
                        if (m_pendingPublishCallback) {
                            auto cb = m_pendingPublishCallback;
                            m_pendingPublishCallback = nullptr;
                            QString hash = data.size() > 1 ? data[1].toString() : QString();
                            cb(hash);
                        }
                    });
                // Errors short-circuit the same callback with empty string +
                // a setError. data[2] is the error message per the header.
                m_deliveryClient->onEvent(m_deliveryObject, "messageError",
                    [this](const QString&, const QVariantList& data) {
                        if (m_pendingPublishCallback) {
                            auto cb = m_pendingPublishCallback;
                            m_pendingPublishCallback = nullptr;
                            QString err = data.size() > 2 ? data[2].toString() : QStringLiteral("unknown");
                            setError("broadcast", err);
                            cb(QString());
                        }
                    });
            }
        }
    }
}

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

// ─── Storage / Delivery integration via LogosAPI ────────────────────────────
//
// Both methods invoke the corresponding module's Q_INVOKABLE method via
// LogosAPIClient::invokeRemoteMethodAsync, then rely on the event handlers
// installed in the constructor (storageUploadDone, messageSent/Error) to
// fire the per-call callback.
//
// Single-flight guarantee: the QML's "publish" button is disabled while
// busy, so we never have two pending upload/broadcast callbacks at once.
// The m_pendingUploadCallback / m_pendingPublishCallback slots reflect that
// invariant — they hold ONE callback at a time.

void WhistleblowerBackend::uploadToStorage(
    const QString& filePath,
    std::function<void(QString)> onComplete)
{
    if (!m_api || !m_storageClient || !m_storageObject) {
        setError("upload", "storage_module not available — running outside Basecamp host?");
        onComplete(QString());
        return;
    }
    if (m_pendingUploadCallback) {
        setError("upload", "another upload already in flight");
        onComplete(QString());
        return;
    }
    m_pendingUploadCallback = onComplete;

    // Invoke storage_module.uploadUrl(QUrl, chunkSize). The synchronous
    // return is a LogosResult — completion comes via storageUploadDone.
    QVariantList args{
        QVariant::fromValue(QUrl::fromLocalFile(filePath)),
        QVariant::fromValue(64 * 1024),
    };
    m_storageClient->invokeRemoteMethodAsync(
        "storage_module", "uploadUrl", args,
        [this](QVariant result) {
            // result is the LogosResult of the sync call. Failure here means
            // the upload couldn't even be queued — clear the pending callback
            // and surface the error.
            // We deliberately don't inspect LogosResult fields; if the upload
            // queued OK we wait for storageUploadDone. If queueing failed,
            // the event won't fire and we'd timeout — handled below by the
            // safety timeout.
            Q_UNUSED(result);
        });

    // Safety timeout: if storageUploadDone doesn't fire in 60s, clear the
    // pending callback and surface a timeout error. Real production would
    // want a longer timeout for big files.
    QTimer::singleShot(60'000, this, [this]() {
        if (m_pendingUploadCallback) {
            auto cb = m_pendingUploadCallback;
            m_pendingUploadCallback = nullptr;
            setError("upload", "timed out waiting for storageUploadDone (60s)");
            cb(QString());
        }
    });
}

void WhistleblowerBackend::broadcastEnvelope(
    const QString& topic,
    const QByteArray& envelopeBytes,
    std::function<void(QString)> onComplete)
{
    if (!m_api || !m_deliveryClient || !m_deliveryObject) {
        setError("broadcast", "delivery_module not available — running outside Basecamp host?");
        onComplete(QString());
        return;
    }
    if (m_pendingPublishCallback) {
        setError("broadcast", "another publish already in flight");
        onComplete(QString());
        return;
    }
    m_pendingPublishCallback = onComplete;

    // delivery_module.send(topic: QString, payload: QString). Per the header
    // the payload is a QString (base64 is fine — receivers decode), so we
    // wrap our envelope bytes in base64 to survive QString round-trip.
    QString payload = QString::fromLatin1(envelopeBytes.toBase64());
    QVariantList args{topic, payload};
    m_deliveryClient->invokeRemoteMethodAsync(
        "delivery_module", "send", args,
        [this](QVariant result) {
            Q_UNUSED(result);
        });

    // Safety timeout (30s — broadcasts should be fast).
    QTimer::singleShot(30'000, this, [this]() {
        if (m_pendingPublishCallback) {
            auto cb = m_pendingPublishCallback;
            m_pendingPublishCallback = nullptr;
            setError("broadcast", "timed out waiting for messageSent (30s)");
            cb(QString());
        }
    });
}
