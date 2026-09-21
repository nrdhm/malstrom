#!/usr/bin/env bash
# Query malstrom test traces from Tempo.
#
# What it does:
#   1. Lists traces exported by tests, filtered by `service.name` or by an
#      explicit TraceQL query.
#   2. Summarizes one trace by ID: every span's wall time, tracing busy/idle
#      split, thread, and log events.
#   3. Lists the service names Tempo has seen (useful to confirm test data
#      actually arrived).
#
# The endpoint used is Tempo's JSON HTTP API, not the OTLP ingest endpoint
# (`/v1/traces` only accepts POST and cannot be queried). The default base URL
# is the Tempo instance used by the malstrom operator tests.
#
# Usage:
#   scripts/tempo-search.sh                         list recent test traces
#   scripts/tempo-search.sh trace <traceID>         summarize one trace
#   scripts/tempo-search.sh services                list known service names
#
# Options:
#   --api URL       Tempo HTTP API base (default: $TEMPO_API or http://192.168.0.233:3200)
#   --service NAME  filter by service.name (default: malstrom-operators-tests)
#   --hours N       look back N hours (default: 72)
#   --limit N       max number of traces returned (default: 50)
#   --q QUERY       TraceQL query; overrides the --service tag filter
#
# Examples:
#   scripts/tempo-search.sh
#   scripts/tempo-search.sh --hours 24 --limit 10
#   scripts/tempo-search.sh --q '{ resource.service.name = "malstrom-operators-tests" && duration > 1s }'
#   scripts/tempo-search.sh trace 780820c6fd9772e72eb6b8a7a2d0e0a4
#
# Requires: curl and jq.

set -euo pipefail

API="${TEMPO_API:-http://192.168.0.233:3200}"
SERVICE="${TEMPO_SERVICE:-malstrom-operators-tests}"
HOURS=72
LIMIT=50
QUERY=""
TRACE_ID=""
CMD="search"

usage() {
    sed -n '2,35p' "$0" | sed 's/^# \{0,1\}//'
}

args=("$@")
i=0
while [ "$i" -lt "${#args[@]}" ]; do
    a="${args[$i]}"
    case "$a" in
        search | trace | services)
            CMD="$a"
            i=$((i + 1))
            ;;
        --api)
            API="${args[$((i + 1))]}"
            i=$((i + 2))
            ;;
        --service)
            SERVICE="${args[$((i + 1))]}"
            i=$((i + 2))
            ;;
        --hours)
            HOURS="${args[$((i + 1))]}"
            i=$((i + 2))
            ;;
        --limit)
            LIMIT="${args[$((i + 1))]}"
            i=$((i + 2))
            ;;
        --q)
            QUERY="${args[$((i + 1))]}"
            i=$((i + 2))
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            if [ "$CMD" = trace ] && [ -z "$TRACE_ID" ]; then
                TRACE_ID="$a"
                i=$((i + 1))
            else
                echo "unknown option: $a" >&2
                echo "run '$0 --help' for usage" >&2
                exit 2
            fi
            ;;
    esac
done

NOW=$(date +%s)
START=$((NOW - HOURS * 3600))

if [ "$CMD" = trace ]; then
    if [ -z "$TRACE_ID" ]; then
        echo "missing traceID" >&2
        exit 2
    fi
    curl -fsS "$API/api/v2/traces/$TRACE_ID" | jq -r '
        def v: (.value | to_entries[0].value);
        .trace.resourceSpans[].scopeSpans[].spans[]
        | [ .name,
            (((.endTimeUnixNano | tonumber) - (.startTimeUnixNano | tonumber)) / 1e6 | tostring) + "ms",
            (([.attributes[]? | select(.key == "busy_ns")][0] | v | tonumber) / 1e6 | tostring) + "ms busy",
            (([.attributes[]? | select(.key == "idle_ns")][0] | v | tonumber) / 1e6 | tostring) + "ms idle",
            (([.attributes[]? | select(.key == "thread.name")][0] | v) // ""),
            ([.events[]?.name] | join("; "))
          ] | @tsv
    '
    exit 0
fi

if [ "$CMD" = services ]; then
    curl -fsS -G "$API/api/v2/search/tag/.service.name/values" \
        --data-urlencode "start=$START" \
        --data-urlencode "end=$NOW" | jq -r '.tagValues[].value' | sort -u
    exit 0
fi

# search
if [ -n "$QUERY" ]; then
    curl -fsS -G "$API/api/search" \
        --data-urlencode "start=$START" \
        --data-urlencode "end=$NOW" \
        --data-urlencode "limit=$LIMIT" \
        --data-urlencode "q=$QUERY" | jq -r '
            .traces[]
            | [ .traceID, .rootServiceName, .rootTraceName,
                (.startTimeUnixNano | tonumber / 1e9 | strftime("%Y-%m-%dT%H:%M:%SZ")),
                ((.durationMs // 0) | tostring) + "ms" ] | @tsv
        '
else
    curl -fsS -G "$API/api/search" \
        --data-urlencode "start=$START" \
        --data-urlencode "end=$NOW" \
        --data-urlencode "limit=$LIMIT" \
        --data-urlencode "tags=service.name=$SERVICE" | jq -r '
            .traces[]
            | [ .traceID, .rootServiceName, .rootTraceName,
                (.startTimeUnixNano | tonumber / 1e9 | strftime("%Y-%m-%dT%H:%M:%SZ")),
                ((.durationMs // 0) | tostring) + "ms" ] | @tsv
        '
fi