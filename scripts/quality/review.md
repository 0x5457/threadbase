# Semantic review pass

Layer 3 of the code-health loop: the tools above measure and nominate, this pass
decides. Read `.quality/` after running `scripts/quality/measure.sh`, then judge
each candidate against the real call chain.

## Ground rules

1. **Measurements are facts, candidates are guesses, verdicts need evidence.**
   Never report a verdict without the call chain that supports it (file, line,
   who imports it, who calls it, what breaks if it goes).
2. **Not every candidate is a defect.** Report `keep` with a reason when the
   tool is right about the fact and wrong about the conclusion.
3. **Never edit generated code.** `web/src/api/types/**` is emitted by ts-rs from
   the Rust crates. A dead DTO is a question about `#[ts(export)]` on the Rust
   type, not an invitation to delete TypeScript.
4. **Deleting is only done once the gates agree:** `pnpm typecheck`,
   `pnpm doctor:ci`, `pnpm build` all pass. No exceptions, no "will fix later".
5. **Prefer shrinking surface over moving code.** Removing an `export` that
   nothing imports costs nothing; a new shared abstraction costs a permanent
   indirection.

## Verdicts

| verdict | meaning | action |
|---|---|---|
| `delete` | no caller, no contract, no plan | remove the symbol and its now-unused imports |
| `inline` | used only inside its own file, exported by habit | drop the `export` keyword |
| `merge` | two clones that must change together | extract one implementation, keep the diff small |
| `keep` | tool is right, conclusion is wrong | record why in this file, do not delete |
| `defer` | real finding, needs a design decision | name the decision, do not guess |

## Known noise in this repository

- **Static entry points**: `public/sw.js` (registered from `lib/push.ts`) and
  `public/theme-boot.js` (inline `<script>` in `index.html`) are not imported.
  `knip.json` lists both as entries.
- **Orphan-by-pivot**: a file can be dead because the feature it served was
  replaced. `SubagentList.tsx` and `cancelSubagent` went away this way once
  `SubagentThreadView` took over. Check git history before assuming a mistake.
- **Symmetric API wrappers**: `src/api/*` mirrors the Rust routes one function
  per endpoint. An unused wrapper usually means the UI has no affordance yet,
  which is a product question, not dead code.
- **Clone pairs that are not duplication**: parallel tables and per-variant
  rows in a `switch` read better duplicated than parameterised.

## Report format

Write `.quality/review.md`:

```md
# Review <date>

## delete
- `symbol` (file:line) — <evidence: no importer, no caller, added in <commit>>
## inline
- `symbol` (file:line) — <evidence: only referenced in its own file>
## merge
- `pathA` ↔ `pathB` — <what the shared shape is, why they diverge later>
## keep
- `symbol` — <why the tool's fact does not imply deletion>
## defer
- `symbol` — <the decision that is missing, and who has to make it>
```

Order candidates by cost of leaving them alone, not by the order the tools
printed them. Cap the review at the top 10: unbounded review is how this layer
stops being read.
