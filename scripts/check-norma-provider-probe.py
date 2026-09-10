#!/usr/bin/env python3
"""Offline safety check for the live provider probe; never starts a CLI."""
from pathlib import Path
import runpy
import shlex


def check_command_validation():
    """Exact shell wrapping must not broaden the approved operation.

    Data: one fixed write command; wrappers contain three arguments: shell,
    one execution flag, and the exact command. Malicious forms add a fourth
    argument or append another shell operation inside or outside the payload.
    Flow: load the probe without running main, then check accepted and denied
    representations without executing any of them.
    Expected: direct and quoted shell wrappers match; altered writes, extra
    operations, unknown shells, and malformed quoting never match.
    """
    probe = runpy.run_path(str(Path(__file__).with_name("probe-norma-mobile-providers.py")))
    matches = probe["matches_probe_command"]
    command = "printf NORMA_APPROVAL_OK > /tmp/disposable/permission-check.txt"
    wrapped = "/bin/zsh -lc " + shlex.quote(command)
    assert matches(command, command), "The exact direct command must match"
    assert matches(wrapped, command), "A quoted zsh wrapper must preserve the exact operation"
    assert matches(["/bin/zsh", "-lc", command], command), "The exact argv wrapper must match"
    assert not matches(wrapped + "; touch /tmp/extra", command), "A second outer operation must be rejected"
    assert not matches(["/bin/zsh", "-lc", command + "; touch /tmp/extra"], command), "An extra inner operation must be rejected"
    assert not matches(["/bin/zsh", "-lc", command, "extra"], command), "Additional shell arguments must be rejected"
    assert not matches(["/tmp/custom-shell", "-lc", command], command), "An unknown shell must be rejected"
    assert not matches(command.replace("permission-check", "other"), command), "A different target must be rejected"
    assert not matches("/bin/zsh -lc '", command), "Malformed quoting must be rejected"
    assert not matches(None, command), "Missing command data must be rejected"
    print("Provider probe command validation: passed")


if __name__ == "__main__":
    check_command_validation()
