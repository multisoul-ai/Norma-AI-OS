# SPEC：软件 License 硬门控接入（Keygate）

> 2026-10-24 归档说明：本文涉及的 spike 源码已按用户确认移除；以下为历史记录，原文件可从 Git 提交 `0aed07c2` 查阅。现行入口见 `docs/README.md`。

日期：2026-07-14  
状态：ready-for-agent（grilling 已定案；Keygate spike GO）  
设计：已并入本文。原文见 git 历史 `docs/design-docs/2026-07-14-software-licensing.md`。  
ADR：[`0014`](../adr/0014-spike-keygate-before-app-licensing.md)（spike GO）、[`0015`](../adr/0015-license-hard-gate-outside-workbench.md)（硬门控 + GateRoot）  
Spike：[`spikes/keygate-mvp/README.md`](../README.md#history)

> 历史：见 git 提交。原文路径：`docs/design-docs/2026-07-14-software-licensing.md`。

产品模型补充：一期不控设备；人工在后台发证。买断、首次联网激活后离线验签、支付与授权解耦，以及 Keygate spike GO，仍以本文和 ADR 0014、0015 为准。本文状态仍是 ready-for-agent，不表示硬门控已经上线。

## Problem Statement

Norma OS 需要可商业化的买断激活。运营者已能在自托管 Keygate 手动发证；终端用户还不能在 App 内粘贴 License Key 完成首次联网激活，也无法在之后断网使用。同时产品要求：**未激活不得进入工作台**（画布 / Terminal / Settings 等），避免「软提示但仍可完整使用」无法支撑付费。

## Solution

用户打开 App 后，若本地无有效离线凭证且未设置开发跳过，则只看到激活壳（粘贴 Key、激活、退出）。激活时联网向 Keygate 换取签名 offline token 并落盘；之后启动在断网下仍可凭内嵌公钥与绑机校验进入工作台。开发者可用环境变量跳过门控。支付与港企收款不在本期。

## User Stories

1. As a paying user, I want to paste my License Key on first launch, so that I can activate the product without creating an account.
2. As a paying user, I want activation to require network only the first time (or when the offline token expires), so that I can keep working offline afterward.
3. As a paying user, I want the app to open straight into the workbench after a successful activation, so that I do not repeat the activation flow every launch.
4. As a paying user, I want a clear error when my Key is invalid or the license server is unreachable, so that I know whether to retry, check the Key, or come back online.
5. As a paying user, I want token expiry to return me to the activation shell with a clear “need network to renew” message, so that a buyout license can mint a fresh offline token without feeling like the purchase expired.
6. As a paying user, I want to quit from the activation shell without entering the workbench, so that I am not forced into the product unactivated.
7. As an unlicensed user, I want to be unable to reach the canvas, terminals, or settings, so that the product is not fully usable without a license.
8. As an unlicensed user, I want the activation shell to be the only primary UI, so that the path to activate is obvious.
9. As a user who copied an offline token from another Mac, I want activation to fail local checks, so that a token bound to another machine cannot unlock my machine.
10. As a user who tampered with the offline token file, I want the app to treat me as unactivated, so that signature integrity is enforced.
11. As a user who deleted the local token file, I want to see the activation shell again, so that I can re-enter my Key and reactivate.
12. As an operator, I want to issue perpetual licenses from Keygate admin, so that I can deliver Keys manually after offline payment.
13. As an operator, I want client activation to call the same Keygate activate API validated in the spike, so that server-side issuance and client consumption stay aligned.
14. As a developer, I want `NORMA_SKIP_LICENSE=1` to open the workbench immediately, so that daily `cargo run` is not blocked by Keygate.
15. As a developer, I want `NORMA_LICENSE_URL` to point at a local Keygate, so that I can test real activate against the spike stack.
16. As a developer, I want license crypto and gate evaluation in a GPUI-free crate, so that I can unit-test activation rules without opening a window.
17. As a maintainer, I want the public verifying key embedded in the app binary, so that replacing a on-disk pubkey file cannot mint forged offline tokens.
18. As a maintainer, I want the License Key itself not stored on disk, so that re-activation requires pasting the Key again and local theft of the Key string is reduced.
19. As a maintainer, I want activate HTTP to run off the GPUI foreground path, so that typing and UI stay responsive during activation.
20. As a maintainer, I want `MultisoulApp` (Runtime, PTY, agent hooks, etc.) constructed only after the gate allows the workbench, so that an unactivated process does not start live node infrastructure.
21. As a maintainer, I want a single window that switches root from activation shell to workbench, so that quit/last-window behavior stays simple on macOS.
22. As a maintainer, I want machine hardware identity sent as Keygate `identifier` and checked against token `did` offline, so that device binding is consistent end-to-end even without server-side seat enforcement.
23. As a QA engineer, I want automated tests on gate evaluation (valid / expired / bad signature / wrong `did` / skip), so that hard-gate regressions are caught without manual runs.
24. As a future commercial owner, I want payments to remain decoupled, so that Lemon Squeezy / HK entity work can land later without rewriting the client gate contract.

## Implementation Decisions

- **Hard gate, activation shell only**: Unactivated users never enter the workbench UI. No soft “use anyway” path except the explicit skip env var.
- **Architecture — GateRoot outside workbench**: Boot opens one window whose root is a gate entity with two phases: Activation shell, or Workbench. `MultisoulApp` is constructed only when entering Workbench (or when skip is set). Rejected: adding `AppScreen::Activation` inside an already-constructed `MultisoulApp` (would start Runtime/PTY/servers while unactivated).
- **Same-window root switch**: Activation success replaces the root with Workbench in the same window; do not close an activation window and open a second workbench window.
- **New domain crate `license` (GPUI-free)**: Owns offline token parse/verify, embedded pubkey verification, expiry, `did` binding check, and activate request/response DTOs. No SQLite, no GPUI.
- **App adapter responsibilities**: Read/write offline token under the app data directory (`…/license/offline.token`), perform HTTP activate on a background executor, supply macOS hardware machine id, read `NORMA_SKIP_LICENSE` / `NORMA_LICENSE_URL`, host activation shell UI (prefer `ui::component`).
- **Embedded pubkey**: The Ed25519 public key corresponding to Keygate `LICENSE_SIGNING_KEY` is compiled into the client. On-disk pubkey files are not the trust anchor (spike stored pubkey for convenience only).
- **Token storage**: Persist only the offline token file under Application Support (via existing app data dir helper). Do not store the License Key. Do not put token/verify in SQLite for v1. Keychain deferred.
- **Keygate URL**: Compile-time default base URL plus `NORMA_LICENSE_URL` override. Do not expose server URL in Settings.
- **Activate API contract** (aligned with spike): `POST /api/v1/license/activate` with license key, hardware `identifier`, `identifier_type: device`; persist returned token; treat `activated` / `already_activated` as success when token present.
- **Machine binding**: Activate sends hardware fingerprint as `identifier`. Offline evaluate requires payload `did` to equal current machine id; mismatch → unactivated (activation shell).
- **Token expiry**: Expired or otherwise invalid token → activation shell; user must paste Key again and renew online. Buyout refers to license entitlement on the server, not an immortal offline blob.
- **Dev skip**: `NORMA_SKIP_LICENSE=1` (or equivalent documented name) bypasses evaluate and constructs Workbench directly.
- **No Settings License page in this spec**: Status / clear / re-paste management deferred.
- **Spike remains reference**: `spikes/keygate-mvp/` stays disposable documentation and local server harness; production logic lives in `license` + `app`, not by shelling out to spike scripts.
- **Last-window quit**: Existing macOS rule unchanged — closing the last window quits the process, including from the activation shell.

## Testing Decisions

### Primary seam (prefer this one)

**`license` public gate evaluation** — a pure function (name TBD, e.g. `evaluate_local_license`) that, given token bytes (or absence), embedded pubkey, current machine id, and clock, returns a structured status: activated vs invalid reason (missing / bad format / bad signature / expired / did mismatch / bad status).

- Test **external behavior** of that API only: inputs → status. Do not assert on HTTP, GPUI, or file paths inside `license` unit tests.
- Cover: valid token; tampered payload; wrong pubkey; expired `exp`; `did` mismatch; missing token; skip is **not** part of `license` (skip is an app/env concern above the seam).

### Secondary seam (thin)

**App boot phase decision** — a small pure helper (or GateRoot state transition test) mapping `(skip, evaluate_status) → Activation | Workbench`. Keeps UI wiring dumb.

### Activate HTTP

- Prefer a narrow port/trait for “POST activate → token bytes” so tests can inject a fake without real Keygate.
- One integration path may hit local Keygate (spike compose) as ignored/manual if needed; not required for every CI run.

### Prior art

- Spike: `spikes/keygate-mvp/tests/test_offline_verify.py` (signature / expiry / tamper).
- App: `AppScreen` / workspace flow unit tests for screen transitions; e2e L1 for user-visible boot when hard gate is observable in headless GPUI.
- Domain crates: GPUI-free `cargo test -p license` mirrors `canvas` / `runtime` style (`docs/quality/test-requirements.md`).

### What “good” means here

- Assert user-visible or API-visible outcomes (shell vs workbench eligibility, error kinds), not private layout of GateRoot fields.
- Hard-gate acceptance must include negative cases (tamper, wrong machine, expiry), not only happy-path activate.

## Out of Scope

- Payments, invoices, Lemon Squeezy, HK company / Path 2
- Settings → License management UI
- Storing License Key locally or in Keychain
- Server-side seat / max-activation enforcement productization (plan may remain high-cap)
- Full feature-level soft locks inside an already-open workbench
- Online check on every launch
- Customer portal / account system
- Replacing Keygate with Keygen or DIY signing (spike already GO on Keygate)
- Changing Keygate upstream bugs beyond client workarounds already documented in the spike

## Further Notes

### Glossary pointers

Use `CONTEXT.md` terms: **硬门控**, **激活壳**, **离线凭证 / offline token**, **首次联网激活**, **GateRoot**, **工作台** (`MultisoulApp`). Avoid calling this phase “soft thin integrate” — that wording is superseded.

### Env vars (client)

| Variable | Role |
|----------|------|
| `NORMA_SKIP_LICENSE` | When truthy, skip gate → workbench |
| `NORMA_LICENSE_URL` | Override Keygate base URL |

Exact truthy parsing to be fixed in implementation; document in README / packaging notes when shipping.

### Acceptance checklist (agent)

| # | Criterion | Evidence |
|---|-----------|----------|
| AC1 | No valid token and no skip → activation shell only; workbench not constructed | `license` + app/e2e |
| AC2 | Valid Key + network activate → workbench; relaunch offline still workbench | tests + manual against spike |
| AC3 | Tamper / expiry / `did` mismatch → activation shell | `cargo test -p license` |
| AC4 | Skip env → workbench without Keygate | app unit / e2e |
| AC5 | Activate I/O not on GPUI foreground | code review + performance/async norms in AGENTS.md |

### Suggested ticket split (for `/to-tickets`)

1. `license` crate + evaluate seam + embedded pubkey wiring  
2. App GateRoot + activation shell UI + token file IO + skip/URL env  
3. Background HTTP activate + success transition to `MultisoulApp`  
4. e2e / app tests for AC1–AC4  

Blocking: (1) before (2–3); (2) before (4); (3) before (4) for activate happy path.
