"""Fail closed unless the process is already forbidden to create TCP sockets."""

import errno
import socket


def verify_network_denied():
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as connection:
            connection.settimeout(0.2)
            connection.connect(("127.0.0.1", 9))
    except OSError as error:
        if error.errno in (errno.EPERM, errno.EACCES):
            return {"network_denied": True, "probe": "local TCP denied", "errno": error.errno}
        raise RuntimeError("network denial was not verified; use run-offline.sh") from error
    raise RuntimeError("network access allowed; use run-offline.sh")
