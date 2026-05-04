#pragma once

#include <QObject>
#include <QString>
#include <QTimer>
#include <QJsonObject>
#include <functional>

class LogosAPI;
class LogosAPIClient;
class LogosObject;

/**
 * WhistleblowerBackend — Qt-side controller for the LP-0017 plugin.
 *
 * Bridges three things:
 *   1. The QML UI (file picker, metadata form, "publish" / "anchor" buttons)
 *   2. The Logos Core storage_module + delivery_module (via LogosAPI*)
 *   3. The Rust FFI layer (whistleblower_anchor_one, etc.) for the on-chain
 *      registry path
 *
 * The LogosAPI handle gives us in-process access to the Q_INVOKABLE methods
 * on storage_module and delivery_module, which Basecamp has already loaded.
 * That is dramatically simpler than running our own logoscore subprocess
 * (the headless subprocess approach was the alternative we evaluated and
 * rejected — see adapters/logos/README.md for the trade-off).
 */
class WhistleblowerBackend : public QObject {
    Q_OBJECT

    // --- exposed to QML ---
    Q_PROPERTY(QString selectedFile READ selectedFile NOTIFY selectedFileChanged)
    Q_PROPERTY(QString lastCid READ lastCid NOTIFY lastCidChanged)
    Q_PROPERTY(qint64 lastAnchorTimestamp READ lastAnchorTimestamp NOTIFY lastAnchorTimestampChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)
    Q_PROPERTY(QString busyMessage READ busyMessage NOTIFY busyChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    /// Stage indicator for the QML progress bar:
    /// 0 = nothing; 1 = uploading; 2 = broadcasting; 3 = anchored.
    Q_PROPERTY(int stage READ stage NOTIFY stageChanged)

public:
    explicit WhistleblowerBackend(LogosAPI* api, QObject* parent = nullptr);
    ~WhistleblowerBackend() override;

    // --- getters ---
    QString selectedFile() const { return m_selectedFile; }
    QString lastCid() const { return m_lastCid; }
    qint64 lastAnchorTimestamp() const { return m_lastAnchorTimestamp; }
    QString lastError() const { return m_lastError; }
    QString busyMessage() const { return m_busyMessage; }
    bool busy() const { return m_busy; }
    int stage() const { return m_stage; }

public slots:
    /// QML calls this when the user picks a file.
    void setSelectedFile(const QString& filePath);
    /// QML calls this when the user clicks "Publish".
    /// Drives the upload (storage_module) -> broadcast (delivery_module)
    /// pipeline; on success, lastCid is set and the user can then click
    /// "Anchor on chain" to trigger the on-chain step.
    void publish(const QString& title, const QString& description, const QString& tagsCsv);
    /// QML calls this when the user clicks "Anchor on chain". Calls the
    /// Rust FFI's whistleblower_anchor_one with the stored lastCid +
    /// last metadata_hash.
    void anchorLast();

signals:
    void selectedFileChanged();
    void lastCidChanged();
    void lastAnchorTimestampChanged();
    void lastErrorChanged();
    void busyChanged();
    void stageChanged();
    /// Fine-grained pipeline events for the QML toast.
    void uploadComplete(const QString& cid);
    void broadcastComplete(const QString& messageHash);
    void anchorComplete(const QString& cidHash);
    void error(const QString& stage, const QString& msg);

private:
    void setBusy(bool busy, const QString& message);
    void setStage(int s);
    void setError(const QString& stage, const QString& msg);

    /// Upload a file to Logos Storage. Async — calls the storage module's
    /// uploadUrl Q_INVOKABLE method, listens for storageUploadDone signal,
    /// invokes `onComplete(cid)` on success or sets error on failure.
    void uploadToStorage(const QString& filePath, std::function<void(QString)> onComplete);

    /// Broadcast a (CID + metadata) envelope to the LP-0017 delivery topic.
    /// Calls delivery_module's send Q_INVOKABLE method.
    void broadcastEnvelope(
        const QString& topic,
        const QByteArray& envelopeBytes,
        std::function<void(QString)> onComplete);

    /// Compute metadata_hash via the Rust FFI's compute_metadata_hash.
    /// Returns the hex string + the canonical envelope bytes (base64) so we
    /// can both broadcast and anchor with the same hash.
    bool computeEnvelope(
        const QString& cid,
        const QString& title,
        const QString& description,
        const QString& contentType,
        qint64 sizeBytes,
        const QStringList& tags,
        QString* outMetadataHashHex,
        QByteArray* outEnvelopeBytes);

    /// Submit anchor_one via the Rust FFI. Blocking (the FFI uses its own
    /// tokio runtime); call this from a worker thread.
    QJsonObject anchorOneFfi(const QString& cid, const QString& metadataHashHex);

    QJsonObject baseFfiArgs() const;

    LogosAPI* m_api {nullptr};
    /// Resolved by the constructor via m_api->getClient(...). Null when the
    /// plugin runs outside Basecamp (standalone preview app).
    LogosAPIClient* m_storageClient {nullptr};
    LogosAPIClient* m_deliveryClient {nullptr};
    /// LogosObject* handles obtained via client->requestObject(...). Used as
    /// the originObject argument to client->onEvent(...).
    LogosObject* m_storageObject {nullptr};
    LogosObject* m_deliveryObject {nullptr};

    /// Single-flight callbacks for in-progress storage upload / delivery
    /// publish. Cleared when the corresponding event fires (storageUploadDone
    /// / messageSent / messageError) or when the safety timeout elapses.
    std::function<void(QString)> m_pendingUploadCallback;
    std::function<void(QString)> m_pendingPublishCallback;

    QString m_walletPath;
    QString m_sequencerUrl;

    QString m_selectedFile;
    QString m_lastCid;
    QString m_lastMetadataHashHex;
    qint64  m_lastAnchorTimestamp {0};

    QString m_lastError;
    QString m_busyMessage;
    bool    m_busy {false};
    int     m_stage {0};
};
