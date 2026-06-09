# LP-0017 voiceover script — Whistleblower document publishing and indexing

Read this alongside `scripts/record-final-video.sh`. Keep tone factual. Do not claim live GUI upload/broadcast unless you actually perform it.

If you are currently recording `bash scripts/verify-testnet.sh`, use `AUDIO_SCRIPT_CURRENT_VERIFY_TESTNET.md` instead. That file matches the read-only public-testnet hash re-query and PDA decode flow exactly.

## Opening

"This is LP-0017: a Whistleblower document publishing and indexing submission for Logos. The repository contains a Basecamp app surface, a reusable document-indexing module, Delivery metadata envelopes, optional batch anchoring, and public-testnet registry verification. The goal is to show that document publication metadata can be stored, broadcast, indexed, and anchored in a way evaluators can reproduce."

## Scene 1 — Repository state and validators

"First I am showing the exact repository revision and the local validation gates. These checks verify that the submission documents and demo artifacts are present and internally consistent before I show the functional evidence."

## Scene 2 — Success criteria

"This checklist maps the implementation to the prize requirements: Storage-backed document bytes, Delivery metadata envelopes, optional on-chain anchoring, permissionless batch anchoring, query-by-CID registry behavior, reusable indexing API, Basecamp package metadata, IDL artifacts, public-testnet evidence, and honest performance notes."

## Scene 3 — Storage, Delivery, and indexing surfaces

"Here I am showing the app-facing integration surfaces. The Basecamp manifest declares its module dependencies. The sample envelope includes CID, title, description, content type, byte size, timestamp, and tags. The indexing API is separate from the app UI so other Logos modules can reuse it."

## Scene 4 — Demo script modes

"The demo script supports multiple evaluator paths: read-only public-testnet verification, permissionless batch anchoring, fresh testnet lifecycle runs, and local sequencer corroboration. The important point is that verification does not depend on trusting this recording."

## Scene 5 — Public-testnet anchor verification

"This is the live public-testnet verification path. It re-queries the public sequencer for deployment evidence, single-anchor evidence, duplicate-anchor handling, and batch-anchor transactions. This is the strongest evidence for the registry behavior."

## Scene 6 — IDL, testnet proof, and compute evidence

"Now I am showing checksums and summaries for the IDL, testnet proof, registry spike, and benchmark files. These files document the on-chain interface, the exact public-testnet evidence, the CID-derived account model, and compute-cost expectations."

## Scene 7 — Basecamp package evidence

"This section verifies the Basecamp package artifacts. It shows metadata and manifests for the Whistleblower plugin, plus Storage and Delivery dependencies. It also checks installed Basecamp profile artifacts."

### Optional local window switch prompt

"I am switching to the local Logos Basecamp window now. What I am showing here is the locally installed Basecamp app/profile after running the package install script on this M4 laptop. If visible, you should see the Whistleblower plugin and the Storage and Delivery modules available to Basecamp. This is package and runtime-availability evidence; the terminal verification remains the authoritative proof for Delivery envelopes and public-testnet anchoring."

Switch back to terminal after showing Basecamp for 5–15 seconds.

## Result

"LP-0017 shows the app source surfaces, reusable indexing module, Delivery metadata envelope, permissionless batch-anchor tool, public-testnet registry evidence, and Basecamp package/install evidence. The submission avoids claiming more than the evidence proves: GUI visibility is package evidence; registry correctness is verified by terminal and public-testnet checks."
