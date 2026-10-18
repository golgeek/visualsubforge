"""Disposable FileChooser service for the private-bus regression harness."""

import json
import os
import signal

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

NAME = "org.freedesktop.portal.Desktop"
XML = """<node><interface name="org.freedesktop.portal.FileChooser">
  <property name="version" type="u" access="read"/>
  <method name="OpenFile">
    <arg type="s" direction="in"/><arg type="s" direction="in"/>
    <arg type="a{sv}" direction="in"/><arg type="o" direction="out"/>
  </method>
</interface></node>"""


def connect():
    return Gio.DBusConnection.new_for_address_sync(
        os.environ["DBUS_SESSION_BUS_ADDRESS"],
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT
        | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None,
        None,
    )


def claim(connection, flags):
    result = connection.call_sync(
        "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
        "RequestName", GLib.Variant("(su)", (NAME, flags)),
        GLib.VariantType.new("(u)"), Gio.DBusCallFlags.NONE, -1, None,
    )
    assert result.unpack() == (1,), result


def main():
    case = os.environ["VSF_PORTAL_TEST_CASE"]
    connection = connect()
    connection.set_exit_on_close(False)
    replacement = None
    requests = 0
    loop = GLib.MainLoop()

    def complete(sender, handle):
        nonlocal replacement
        if case == "restart" and requests == 1:
            connection.close_sync(None)
            replacement = connect()
            register(replacement)
            claim(replacement, 1)
        elif case == "disconnect":
            connection.close_sync(None)
        elif case == "replace":
            replacement = connect()
            claim(replacement, 2)  # REPLACE_EXISTING; original allows replacement.
        elif case == "bus-loss":
            # PID belongs to the bus created by check.py, never the desktop bus.
            os.kill(int(os.environ["VSF_PORTAL_TEST_BUS_PID"]), signal.SIGTERM)
        else:
            code = 1 if case == "cancel" else 0
            values = {} if code else {
                "uris": GLib.Variant("as", ["file:///tmp/caf%C3%A9%20subtitle.srt"])
            }
            active = replacement if case == "restart" else connection
            active.emit_signal(
                sender, handle, "org.freedesktop.portal.Request", "Response",
                GLib.Variant("(ua{sv})", (code, values)),
            )
            active.flush_sync(None)
        print(json.dumps({"event": "completed", "case": case}), flush=True)
        return GLib.SOURCE_REMOVE

    def method(conn, sender, path, interface, name, parameters, invocation):
        nonlocal requests
        requests += 1
        parent, title, options = parameters.unpack()
        try:
            assert name == "OpenFile" and title == "Open Subtitles", (name, title)
            assert options["modal"] and not options["multiple"], options
            expected = [("Subtitles", [(0, "*.srt"), (0, "*.ass"),
                                        (0, "*.ssa"), (0, "*.vtt")])]
            assert options["filters"] == expected, options["filters"]
        except (AssertionError, KeyError) as error:
            # An exception in a Gio callback otherwise leaves the method reply
            # outstanding, hiding contract drift behind the harness timeout.
            message = f"Unexpected FileChooser request: {error}"
            print(json.dumps({"event": "rejected", "message": message}), flush=True)
            invocation.return_dbus_error("org.visualsubforge.Test.InvalidRequest", message)
            return
        handle = (
            "/org/freedesktop/portal/desktop/request/"
            + sender[1:].replace(".", "_") + "/" + options["handle_token"]
        )
        print(json.dumps({"event": "request", "parent": parent, "handle": handle}), flush=True)
        invocation.return_value(GLib.Variant("(o)", (handle,)))
        # Ensure the failure happens after the handle reply, while Response is
        # outstanding. A missing service at startup exercises a different path.
        GLib.timeout_add(100, complete, sender, handle)

    def register(target):
        target.register_object(
            "/org/freedesktop/portal/desktop",
            Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0], method,
            lambda *args: GLib.Variant("u", 4), None,
        )

    register(connection)
    claim(connection, 1)  # ALLOW_REPLACEMENT
    print("READY", flush=True)
    loop.run()


if __name__ == "__main__":
    main()
