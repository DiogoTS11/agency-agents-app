# Tools vs Operational Capabilities

DF AG Agency has two separate registries with different meanings.

## Agent-consumer tools

`src-tauri/data/tools.json` is the upstream-compatible catalog of destinations that can consume Agency Agents output, such as Claude Code, Codex, Gemini CLI, OpenCode, Antigravity, and Hermes.

A row in this catalog means **supported/recognized as an agent target**. Detection still comes from the machine, and app-installability is derived from renderer coverage. Do not add production tools such as FFmpeg, CapCut, Playwright, Vercel, or Remotion here merely because Digital Flow uses them.

## Operational capabilities

`src-tauri/data/capabilities.json` describes capabilities a project may require during `prepare-project`, such as Git, Node.js, FFmpeg, Playwright, deployment services, video workflows, or social-production workflows.

A registry entry is taxonomy only. **It never proves availability.** Runtime status must come from the normalized `EnvironmentEvidence` supplied to `prepare-project`.

The runtime uses project signals to decide whether a capability is required or recommended, then reconciles that requirement with factual evidence:

- `PRESENT` evidence can produce `AVAILABLE` / existing-use semantics.
- probe failure remains `VERIFY`; it is not treated as absence.
- required capabilities with no evidence remain visible as a gap or approval-gated net-new capability according to registry policy.
- optional capabilities with no evidence may remain recommended with `NO_EVIDENCE`, but they are not promoted to factual availability and do not become project gaps merely because they are absent.
- services that are available still require approval before connection where the existing doctrine requires it.

## Boundary

Keep these domains separate:

- **Tools** = where agents can be rendered/deployed.
- **Capabilities** = what a project needs to execute.
- **Agents** = reconciled roles selected from the live corpus.
- **Environment evidence** = factual proof of what exists on the authorized machine.

Do not encode secrets, auth values, provider tokens, cookies, local credentials, or client-sensitive configuration in either registry.
## ARTEMIS Android QA (pilot, 2026-10-18)

`artemis-android-qa` is an **optional operational capability**, not an agent, not an IDE install target and not proof that a device or model is available. `prepare-project` recommends it for Android/APK work and does not route it for websites or iOS-only work. An explicit Artemis request marks it required; missing runtime evidence still requires approval.

Digital Flow OS controls governance in `DigitalFlow/docs/command-center/digital-flow-os-capability-map.md` and tracks the pilot in [DigitalFlow issue #183](https://github.com/DiogoTS11/DigitalFlow/issues/183). The local pilot has passed Python/MCP/UI checks only. **Do not auto-execute** `mobile_run_task`, install global IDE MCP/rules, authorize a device, install Accessibility Helper, call paid models or claim real Android QA. A test needs a connected authorized Android device/emulator, verified costs and a scoped agent integration with rollback. Actual availability must come from factual `EnvironmentEvidence` after approval.