"""Summarize one process trace; optionally correlate PresentMon 2.6 QPC output.

This is offline analysis. It never launches a process or touches a profile.
Gallery primitive IDs are not PresentMon present IDs: correlation is by time
window only, and GPU callback timings must not be used as display timings.
"""

import argparse
from collections import Counter
import csv
import json
import math
from pathlib import Path
import re
from statistics import median


QUOTED = r'"(?:\\.|[^"\\])*"'
TRACE_TOKEN = re.compile(
    QUOTED + r"|\[APPLY-TRACE\] (?:timestamp_unix_ms=(?P<timestamp_unix_ms>\d+) )?"
    r"qpc_ticks=(?P<qpc_ticks>\d+|unavailable) qpc_frequency=(?P<qpc_frequency>\d+|unavailable) "
    r"pid=(?P<pid>\d+) (?:op=(?P<op>none|\d+) )?stage=(?P<stage>[\w-]+)(?=\s|\[|$)"
)
DETAIL_FIELD = re.compile(r"(?:^|\s)([a-z_][a-z0-9_]*)=(" + QUOTED + r"|[^\s]+)")


def metrics(values):
    if not values:
        return {"count": 0}
    ordered = sorted(values)
    return {
        "count": len(values),
        "median": median(values),
        "p95": ordered[math.ceil(len(values) * .95) - 1],
        "max": ordered[-1],
    }


def summarize(trace, pid, presentmon=None, swapchain=None):
    events = []
    for line in trace.splitlines():
        # Concurrent appenders can write several complete payloads before a newline.
        # A quoted filename or diagnostic can also contain header-like text.
        headers = [token for token in TRACE_TOKEN.finditer(line) if token["stage"]]
        for index, header in enumerate(headers):
            if header["pid"] != str(pid):
                continue
            end = headers[index + 1].start() if index + 1 < len(headers) else len(line)
            details = line[header.end():end].rstrip().removesuffix("[DEBUG]")
            fields = dict(DETAIL_FIELD.findall(details))
            fields.update({key: value for key, value in header.groupdict().items() if value is not None})
            events.append(fields)
    if not events:
        raise ValueError("No trace events for the requested PID")
    frequencies = {int(e["qpc_frequency"]) for e in events}
    if len(frequencies) != 1 or min(frequencies) <= 0:
        raise ValueError("Trace requires one positive QPC frequency")
    frequency = frequencies.pop()
    start, end = min(int(e["qpc_ticks"]) for e in events), max(int(e["qpc_ticks"]) for e in events)
    if start >= end:
        raise ValueError("Trace has no elapsed QPC window")
    stages = Counter(e["stage"] for e in events)
    residency = [e for e in events if e["stage"] == "gallery-frame-residency" and "frame" in e]
    callbacks = [int(e["render_to_callback_us"]) for e in events if e["stage"] == "gallery-frame-gpu-complete"]
    report = {
        "pid": pid,
        "qpc_frequency": frequency,
        "window_qpc": [start, end],
        "stage_counts": dict(stages),
        "last_frame_residency": max(residency, key=lambda e: int(e["qpc_ticks"]), default=None),
        "render_to_gpu_callback_us": metrics(callbacks),
        "presentation": {"status": "unmeasured"},
        "limitations": [
            "GPU callback includes polling delay; screenshot is offscreen capture.",
            "Trace logging adds overhead; this is not an uninstrumented performance baseline.",
            "Time-window correlation does not identify individual displayed gallery primitives.",
        ],
    }
    if presentmon is None:
        return report
    reader = csv.DictReader(presentmon.splitlines())
    required = {"ProcessID", "SwapChainAddress", "CPUStartQPC", "DisplayLatency", "DisplayedTime"}
    if not required.issubset(reader.fieldnames or []):
        raise ValueError("Use PresentMon 2.6 with --qpc_time and display tracking")
    rows = [r for r in reader if int(r["ProcessID"]) == pid]
    chains = {r["SwapChainAddress"] for r in rows}
    if swapchain is None:
        if len(chains) != 1:
            raise ValueError("Require exactly one swapchain or explicit --swapchain")
        swapchain = chains.pop()
    rows = [r for r in rows if r["SwapChainAddress"] == swapchain]
    if not rows:
        raise ValueError("No PresentMon rows for PID/swapchain")
    shown = []
    submitted = undisplayed = 0
    for row in rows:
        cpu = int(row["CPUStartQPC"])
        displayed = row["DisplayLatency"] not in ("NA", "") and row["DisplayedTime"] not in ("NA", "")
        if start <= cpu <= end:
            submitted += 1
            undisplayed += not displayed
        if displayed:
            latency = float(row["DisplayLatency"])
            duration = float(row["DisplayedTime"])
            if not math.isfinite(latency) or not math.isfinite(duration) or latency < 0 or duration < 0:
                raise ValueError("Invalid PresentMon display metric")
            display_qpc = cpu + latency * frequency / 1000
            if start <= display_qpc <= end:
                shown.append(display_qpc)
    if not submitted and not shown:
        raise ValueError("PresentMon and trace QPC windows do not overlap")
    shown.sort()
    intervals = [(b - a) * 1000 / frequency for a, b in zip(shown, shown[1:])]
    report["presentation"] = {
        "status": "observed" if shown else "no-displayed-frames",
        "swapchain": swapchain,
        "submitted_in_window": submitted,
        "undisplayed_in_window": undisplayed,
        "displayed_in_window": len(shown),
        "display_interval_ms": metrics(intervals),
    }
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--presentmon", type=Path)
    parser.add_argument("--swapchain")
    args = parser.parse_args()
    try:
        report = summarize(
            args.trace.read_text(encoding="utf-8-sig"), args.pid,
            args.presentmon.read_text(encoding="utf-8-sig") if args.presentmon else None,
            args.swapchain,
        )
    except (ValueError, KeyError, OSError) as error:
        parser.error(str(error))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
