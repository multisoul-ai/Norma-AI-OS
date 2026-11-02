# portable-pty 0.9.0 Windows lifecycle patch

Upstream registry version: 0.9.0. Source commit: `f8921727a11b9f8b073e8c24821d72fd41283500` (`pty/`). The MIT license and original source are retained.

Norma waits for the child independently of ConPTY output EOF. A Windows regression covers abandoning the external reader while the output pipe is full. On pre-24H2 systems, `ClosePseudoConsole` can block while a hidden original read handle remains open and nobody drains it.

The first runner pass records the unmodified upstream destructor order. The candidate change will release `Inner.readable` before `Inner.con`, while the normal reader clone continues draining final output. No dependency version/API upgrade is involved.

Reference: https://learn.microsoft.com/en-us/windows/console/closepseudoconsole
