#!/bin/sh
# exec keeps one child PID for the supervisor to kill and reap.
printf '%s' "$$" > "$VSF_TEST_CHILD_PID"
exec /bin/sleep 60
