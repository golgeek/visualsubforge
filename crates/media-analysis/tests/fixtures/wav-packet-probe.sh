#!/bin/sh
case "$*" in
  *packet=stream_index,pts*)
    printf '%s' "$$" > "$(dirname "$0")/packet.pid"
    exec sleep 60
    ;;
  *) printf '%s' '{"streams":[{"index":0,"codec_name":"pcm_s16le"}],"format":{"format_name":"wav","nb_streams":1}}' ;;
esac
