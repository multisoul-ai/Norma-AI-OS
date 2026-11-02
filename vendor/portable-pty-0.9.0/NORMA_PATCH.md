# portable-pty 0.9.0 Windows lifecycle patch

Upstream registry version: 0.9.0. Source commit: `f8921727a11b9f8b073e8c24821d72fd41283500` (`pty/`). The MIT license and original source are retained.

Norma waits for the child independently of ConPTY output EOF. A Windows regression covers abandoning the external reader while the output pipe is full. On pre-24H2 systems, `ClosePseudoConsole` can block while a hidden original read handle remains open and nobody drains it.

Runner source `796cc43c` reproduced the unmodified upstream deadlock on Windows 2022 (run `41496606263`, job `125453090817`); Windows 2025 returned normally as documented for build 26100+. The patch releases `Inner.readable` before `Inner.con`, while the normal reader clone continues draining final output. No dependency version/API upgrade is involved.

Reference: https://learn.microsoft.com/en-us/windows/console/closepseudoconsole

Norma also owns the complete Windows client process tree. The client is created suspended, assigned to a private, non-inheritable Job Object with `KILL_ON_JOB_CLOSE` and no breakaway permission, and only then resumed. Assignment/resume failure terminates and waits for the suspended root. Killers share ownership of the Job rather than reopening process IDs. Explicit cancellation terminates the Job; root completion terminates remaining descendants and waits for `ActiveProcesses == 0` on the background wait path, preserving the root's original exit code. `try_wait` remains nonblocking. The Future's waiter owns the process handle for the duration of its wait.

Source `c3b3599d` reproduced the previous root-only cancellation on Windows 2022 and 2025 (run `41501997882`): after three seconds the root had exited and both detached descendants were still alive. The regression holds exact process handles and checks published creation times, includes the kill call in its three-second budget, and preserves a separate sentinel. A second case releases a real root to exit with code 7 while the PTY and killers remain owned. Runner verification of the Job patch is required before claiming this lifecycle gate passed.

References: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects and https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject
