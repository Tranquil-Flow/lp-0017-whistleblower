# Document Indexing Adapter API

This crate is the Qt-free reusable boundary for LP-0017. It owns schema,
canonical hashes, retry/dedupe state machines, and durable queue semantics.
Runtime-specific modules own Logos plugin handles and event subscriptions.

## Storage plugin contract

Source: `SPECS/refs/storage_module_plugin.h`.

Relevant calls:

- `uploadUrl(QUrl url, int chunkSize = 64 KiB) -> LogosResult`
- `uploadInit(QString filename, int chunkSize = 64 KiB) -> LogosResult`
- `uploadChunk(QString sessionId, QByteArray chunk) -> LogosResult`
- `uploadFinalize(QString sessionId) -> LogosResult`
- `uploadCancel(QString sessionId) -> LogosResult`

Events:

- `storageUploadProgress` for progress updates.
- `storageUploadDone` for successful CID-bearing completion.

Boundary rule: `StorageClient::upload_file(path)` must not resolve when the
plugin call returns its synchronous `LogosResult`; it resolves only when the
adapter receives the CID-bearing `storageUploadDone` event, or fails on timeout
/ upload error / cancellation.

## Delivery plugin contract

Source: `SPECS/refs/delivery_module_plugin.h`.

Lifecycle:

- `createNode(cfg)` exactly once per node context.
- `start()` before messaging.
- `subscribe(contentTopic)` before receiving.
- `send(contentTopic, payload)` for publication.
- `unsubscribe(contentTopic)` / `stop()` for shutdown.

`send` builds a liblogosdelivery JSON envelope with:

- `contentTopic: string`
- `payload: base64`
- `ephemeral: bool = false`

The Qt plugin accepts payload as a QString and base64-encodes it before crossing
FFI. Received payloads arrive already base64-encoded at the plugin event layer.

Events:

- `messageSent(data[0]=request id, data[1]=message hash, data[2]=ISO timestamp)`
- `messageError(data[0]=request id, data[1]=message hash, data[2]=error, data[3]=ISO timestamp)`
- `messagePropagated(data[0]=request id, data[1]=message hash, data[2]=ISO timestamp)`
- `messageReceived(data[0]=message hash, data[1]=content topic, data[2]=base64 payload, data[3]=ns timestamp)`
- `connectionStateChanged(data[0]=status, data[1]=ISO timestamp)`

Boundary rule: `DeliveryClient::publish(topic, bytes)` resolves only after
`messageSent`. `messagePropagated` is useful telemetry but not final success.
`messageError` must surface retryability so orchestration can decide backoff vs
user-visible failure.

Default topic: `/lp0017-whistleblower/1/cids/json`.

## Registry adapter contract

`RegistryClient` exposes:

- `anchor_one(cid_hash, metadata_hash)`
- `anchor_batch(Vec<(cid_hash, metadata_hash)>)`
- `query_by_cid_hash(cid_hash)`

The implementation may call `spel` CLI or a direct current LEZ client. It must
hide wallet/account transport details from the indexing crate. Duplicate CIDs
must be success/no-op once Task 1.0B proves the final account shape.

## Durable dedupe contract

Delivery does not dedupe for this application. The indexing module computes a
durable envelope hash from `(topic, payload)` and persists it before anchoring.
On restart, the dedupe store is reloaded and duplicate envelopes are skipped.
