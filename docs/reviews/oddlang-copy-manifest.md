# Copy Manifest review

Claude Opus 5.5 reviewed the design and generated companion surface in Claude Code auto mode. Its design review identified distinct validation and parse failures, early size rejection, deferred field and metadata checks, and separate boolean and option tag errors. The implementation preserves all four orders. FileIdentity remains associated with Checkpoint.

The first companion review independently ran 627 native constructions and 2,359 decodes across pinned original and generated libraries. It requested competing invalid fields in the documented corpus and a negative control for early offset handling. The final corpus adds four simultaneous-failure constructions. The repinned mutant both changes the namespace order and defers the size check, which makes truncation win where the original reports Offset.

The final auto review by Claude Opus 5.5 found no blockers in the companion files. It checked the exact framework pin, source-isolated receipt and staged ontology, recomputed the package and caller hashes, and verified the four competing-error cases and two graph mutations. It required publishing the pinned framework commit before companion integration.
