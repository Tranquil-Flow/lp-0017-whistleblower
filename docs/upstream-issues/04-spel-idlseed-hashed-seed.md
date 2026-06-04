# SPEL IdlSeed cannot express a hashed / domain-separated PDA seed

Target repo: `logos-co/spel`
Suggested labels: `enhancement`, `idl`, `correctness`
Severity: moderate (`spel pda` silently derives wrong addresses for the recommended hashed-seed pattern)

---

## Summary

SPEL's `IdlSeed` enum is `const | account | arg` only — there is no variant for
applying a hash to a seed component. Programs that derive PDAs from a
domain-separated hash (the collision-resistant, second-preimage-safe pattern)
cannot express that seed in the IDL, and `spel pda` derives the wrong address
without erroring.

## Concrete case

Our registry entry PDA seed is `sha256(CID_HASH_DOMAIN ‖ cid)`. The closest
expressible IDL form is `{kind: arg, path: cid}`, so `spel generate-idl` emits
that — and `spel pda` then derives from the **raw cid bytes**, not from
`sha256(domain ‖ cid)`. The computed address is wrong, silently.

## Root cause

`IdlSeed` (`spel-framework-core/src/idl.rs`, ~lines 107-116) has no hash variant,
and the `#[account(pda = …)]` attribute only accepts `arg("name")` /
`account("name")` / `const`-style seeds.

## Workaround (what we shipped)

The IDL is generated faithfully for everything SPEL *can* express (instruction
args, account types incl. the entry struct, used by `spel inspect … --type …`),
and the exact seed derivation is documented in the IDL's
`provenance.pda_seed_derivation` field. PDA derivation in code uses our own
`cid_hash()` / `AccountId::for_public_pda(...)`, never `spel pda`.

## Suggested fix

Add a hash seed variant, e.g.:

```rust
IdlSeed::Hash { algo: "sha256", domain: String, components: Vec<IdlSeed> }
```

(or a `hash = sha256(const("…"), arg("cid"))` macro form), and teach `spel pda`
to derive it — so domain-separated-hash PDAs, the secure default, are
expressible and correctly derivable.
