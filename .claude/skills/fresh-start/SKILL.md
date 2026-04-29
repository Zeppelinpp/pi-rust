---
name: fresh-start
description: >
  Use when the user asks "what should I do next", starts a new dev session, or wants a development plan.
  Reads README progress, scans current implementation vs spec, identifies gaps, and produces a prioritized
  architectural plan with a teach-by-guiding breakdown.
---

# Fresh Start — Dev Session Kickoff

When the user signals they want to start working ("what next", "let's continue", "plan for today", etc.),
perform the following steps **in order**. Produce a concise but complete architectural assessment and
a specific, single-step recommendation for the immediate next task.

## Step 1: Read the source of truth

Read these files in parallel:
- `README.md` — pay special attention to the **Progress** and **Roadmap** sections.
- `docs/pi-ai-spec.md`
- `docs/pi-agent-core-spec.md`
- `docs/pi-tui-spec.md`

For each spec, note:
- What the spec says is the **recommended rewrite order**.
- What exists in the current codebase vs what is marked as missing (gaps tables).

## Step 2: Scan the current implementation

Use `Glob` to discover all `.rs` files under `crates/` and read the crate `Cargo.toml`s.
Then read the key source files to understand what is actually implemented:
- `crates/pi-ai/src/lib.rs`, `types.rs`, `message.rs`, `provider.rs`
- `crates/pi-ai/src/providers/*.rs`
- `crates/pi-agent-core/src/lib.rs`
- `crates/pi-tui/src/lib.rs`

Do **not** assume the README is up to date. Verify with the code.

## Step 3: Build the gap analysis

For each crate, summarize the current state in three short sentences or a bullet list:
- **Done**: what types/traits/providers/components are actually in code and compiling.
- **In Progress / Partial**: what exists as skeleton but deviates from the spec or is incomplete.
- **Blocked**: what cannot be started until another crate/module is finished.
- **Missing**: what the spec describes but the codebase has no trace of.

Reference specific spec sections and file paths.

## Step 4: Recommend the single next task

Based on the spec's recommended rewrite order and the actual state of the code, pick **one** concrete next task.
The task should be:
- Small enough to complete in a single focused session (1–3 hours).
- A natural prerequisite for what comes after it.
- Something the user can code themselves with guidance.

State the task in one imperative sentence, then break it down into 3–5 micro-steps.

## Step 5: Deliver in the Fresh Start format

Respond using exactly this structure:

---

### 1. Where we are

A paragraph summarizing the verified codebase state across all three crates. Mention any pleasant surprises (e.g. "`Message` already supports block-level content") and any hard blockers.

### 2. Gap breakdown

| Crate | Done | Partial / Missing | Blocker |
|-------|------|-------------------|---------|
| `pi-ai` | … | … | … |
| `pi-agent-core` | … | … | … |
| `pi-tui` | … | … | … |

### 3. Recommended next step

**Task:** <one-sentence title>

**Why this now?** <2–3 sentences tying the choice to the spec's recommended order and to unblocking downstream work>

**Micro-plan:**
1. …
2. …
3. …
4. …
5. …

### 4. A question to get us moving

Ask the user a leading, Socratic question about the task so they can start reasoning through it. Examples:
- "Looking at the spec's event protocol, what Rust type would you use to represent a stream that yields events and eventually resolves to a final `AssistantMessage`?"
- "Before we write the `FauxProvider`, what invariants should its response queue satisfy so tests can assert on token-level deltas?"
- "The spec says `AgentMessage` should be a superset of `pi-ai::Message`. Would you extend it with an enum wrapper, or would you keep them separate and provide a conversion trait?"

---

## Style constraints

- Be direct and concrete.
- Do not hand the user complete code.
- Cite spec sections (`pi-ai-spec.md §5`, etc.) and file paths (`crates/pi-ai/src/message.rs:42`) whenever relevant.
- If the user asks for the plan but also says they want to jump straight in, treat the question as answered and immediately transition to guiding them through Step 1 of the micro-plan.
- Response IN CHINESE
