#!/usr/bin/env python3
"""Append constants + metadata to a spel-generated IDL.

Reads the `spel generate-idl` JSON (version/name/instructions/accounts) on
stdin and writes the annotated IDL to argv[1]. `spel generate-idl` does not
emit constants or metadata; the constants document the PDA seed derivation
that SPEL's IdlSeed enum cannot express (see BUGS_FILED.md). Invoked by
scripts/regen-idl.sh.
"""
import json
import os
import sys

out_path = sys.argv[1]
idl = json.load(sys.stdin)  # spel-emitted: version, name, instructions, accounts

# CID_HASH_DOMAIN ends with a NUL byte. Build it with chr(0) so this source
# file contains no literal NUL; json.dump emits a valid escape for it.
cid_hash_domain = "lp0017:cid:v1" + chr(0)

idl["constants"] = [
    {
        "name": "CID_HASH_DOMAIN",
        "type": "string",
        "value": cid_hash_domain,
        "comment": "Domain separator for cid_hash. The anchor_one entry PDA seed = "
                   "sha256(CID_HASH_DOMAIN || cid) = whistleblower_core::cid_hash(cid). "
                   "Always re-derived from the canonical CID; never trust an externally "
                   "supplied cid_hash without recomputing.",
    },
    {
        "name": "DEFAULT_CONTENT_TOPIC",
        "type": "string",
        "value": "/lp0017-whistleblower/1/cids/json",
        "comment": "Logos Delivery topic the indexing module publishes envelopes to.",
    },
]

# spel's SpelIdl.metadata is strictly Option<{name, version}> (lssa-lang compat),
# so keep it conformant — otherwise `spel inspect`/`spel pda` reject the IDL.
idl["metadata"] = {
    "name": idl.get("name", "whistleblower_registry"),
    "version": idl.get("version", "0.1.0"),
}

# Generation provenance + the seed-derivation caveat. `provenance` is an
# unknown top-level key, so spel ignores it (serde drops unknown fields); it
# exists for human reviewers.
idl["provenance"] = {
    "spec_version": "lssa-idl/0.1.0",
    "generation": "spel generate-idl (spel %s)" % os.environ.get("SPEL_VERSION", "0.2.0"),
    "source": os.environ.get("DEF", "idl/whistleblower_registry.rs"),
    "regenerate": "bash scripts/regen-idl.sh",
    "deployed_program_id": "54c7f793caa540408ce2ca4c22051d78c466cd5ed8db607feedd19dcb749aa91",
    "note": "instructions + accounts are emitted verbatim by `spel generate-idl`, which "
            "AST-parses idl/whistleblower_registry.rs (it never compiles it). The constants, "
            "metadata, and provenance blocks are appended by scripts/regen-idl.sh. The "
            "production guest (methods/guest/src/bin/whistleblower_registry.rs) stays raw "
            "nssa_core; the parse-only mirror pulls no extra deps into the guest build.",
    "pda_seed_derivation": "anchor_one.entry PDA seed = sha256(CID_HASH_DOMAIN || cid_utf8) "
            "= whistleblower_core::cid_hash(cid). SPEL's IdlSeed enum (const|account|arg) "
            "cannot express a hashed seed, so the emitted seed shows {kind:arg,path:cid} and "
            "`spel pda` will NOT derive the correct address — derive via "
            "whistleblower_core::cid_hash() or the LEZ adapter's entry_pda_for(...). "
            "Tracked upstream — see BUGS_FILED.md.",
}

with open(out_path, "w") as f:
    json.dump(idl, f, indent=2)
    f.write("\n")
print("annotated IDL written to %s" % out_path, file=sys.stderr)
