# Qt Adapter Notes

The Qt/Basecamp backend owns the Logos plugin instances and translates their
asynchronous event model into the Rust adapter traits in `document-indexing`.
The reusable Rust core does not include Qt headers, Qt event loops, or plugin
handles.

## Storage mapping

Trait method: `StorageClient::upload_file(path)`

Possible Qt flows:

1. If the file is addressable as a URL, call `storage_module.uploadUrl(url, chunkSize)`.
2. Otherwise call `uploadInit(filename, chunkSize)`, stream chunks through
   `uploadChunk(sessionId, chunk)`, then `uploadFinalize(sessionId)`.
3. Subscribe to plugin `eventResponse` and map:
   - `storageUploadProgress` -> UI progress telemetry.
   - `storageUploadDone` -> resolve the pending Rust upload future with CID.
   - timeout / cancel / plugin error -> reject with retryable or non-retryable
     `AdapterError`.

Do not report success just because `uploadInit`, `uploadChunk`, `uploadFinalize`,
or `uploadUrl` returned a synchronous `LogosResult`; the CID-bearing event is
the real completion contract.

## Delivery mapping

Trait method: `DeliveryClient::publish(topic, bytes)`

1. Call `delivery_module.send(contentTopic, payloadQString)`.
2. Store the returned request id in a pending map.
3. Resolve the future on `messageSent` with matching `data[0]` request id.
4. Treat `messagePropagated` as progress only.
5. Reject on `messageError` with matching request id.

Trait method: `DeliveryClient::subscribe(topic)`

1. Call `delivery_module.subscribe(contentTopic)`.
2. Map `messageReceived` events to `ReceivedEnvelope`:
   - `data[0]`: message hash
   - `data[1]`: content topic
   - `data[2]`: base64 payload, decoded before entering Rust core
   - `data[3]`: timestamp in nanoseconds since epoch
3. On unsubscribe/drop, call `delivery_module.unsubscribe(contentTopic)`.

## Ownership boundary

Qt/C++ owns:

- Logos module discovery/loading.
- `StorageModulePlugin` and `DeliveryModulePlugin` handles.
- QObject signal connections and thread affinity.
- Base64 decoding at the event boundary.

Rust indexing owns:

- `MetadataEnvelopeV1` schema and canonical hashing.
- CID hash domain separation.
- Upload/broadcast/anchor state machines.
- Durable queue and dedupe behavior.
- Registry adapter trait shape.
