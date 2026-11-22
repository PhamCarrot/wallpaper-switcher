import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("gallery_trace", Path(__file__).with_name("analyze-gallery-trace.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

TRACE = """[APPLY-TRACE] qpc_ticks=1000 qpc_frequency=1000 pid=7 stage=gallery-frame-render frame=1
[APPLY-TRACE] qpc_ticks=1100 qpc_frequency=1000 pid=7 stage=gallery-frame-gpu-complete frame=1 render_to_callback_us=100000
"""
CSV = """ProcessID,SwapChainAddress,CPUStartQPC,DisplayLatency,DisplayedTime
7,0xA,1001,4,10
7,0xA,1011,4,10
7,0xA,1021,NA,NA
8,0xB,1041,4,10
"""


class GalleryTraceTests(unittest.TestCase):
    def test_gpu_completion_alone_is_not_presentation(self):
        result = module.summarize(TRACE, 7)
        self.assertEqual(result["presentation"]["status"], "unmeasured")
        self.assertEqual(result["render_to_gpu_callback_us"]["count"], 1)

    def test_qpc_display_correlation_filters_pid_and_dropped_frames(self):
        result = module.summarize(TRACE, 7, CSV)["presentation"]
        self.assertEqual(result["submitted_in_window"], 3)
        self.assertEqual(result["undisplayed_in_window"], 1)
        self.assertEqual(result["displayed_in_window"], 2)
        self.assertEqual(result["display_interval_ms"]["median"], 10)

    def test_adjacent_log_records_keep_their_process_and_callback(self):
        events = TRACE.splitlines()
        other = events[1].replace("pid=7", "pid=8").replace("qpc_ticks=1100", "qpc_ticks=5000")
        joined = events[0] + "[DEBUG] " + events[1] + other
        result = module.summarize(joined, 7)
        self.assertEqual(result["stage_counts"], {"gallery-frame-render": 1, "gallery-frame-gpu-complete": 1})
        self.assertEqual(result["window_qpc"], [1000, 1100])
        self.assertEqual(result["render_to_gpu_callback_us"]["max"], 100000)

    def test_quoted_diagnostics_cannot_override_header_or_invent_a_record(self):
        trace = TRACE.splitlines()
        diagnostic = ' stderr_prefix="child says pid=99 [APPLY-TRACE] qpc_ticks=9 qpc_frequency=2 pid=7 stage=extra ok"'
        result = module.summarize(trace[0] + diagnostic + trace[1], 7)
        self.assertEqual(result["window_qpc"], [1000, 1100])
        self.assertEqual(result["stage_counts"], {"gallery-frame-render": 1, "gallery-frame-gpu-complete": 1})

    def test_latest_residency_uses_qpc_when_concurrent_lines_arrive_out_of_order(self):
        events = TRACE.splitlines()
        newer = events[1].replace("gallery-frame-gpu-complete", "gallery-frame-residency")
        older = events[0].replace("gallery-frame-render", "gallery-frame-residency")
        result = module.summarize(newer + older, 7)
        self.assertEqual(result["last_frame_residency"]["qpc_ticks"], "1100")

    def test_refuses_ambiguous_or_unrelated_evidence(self):
        for csv in [CSV.replace("0xA,1011", "0xB,1011"), CSV.replace("CPUStartQPC", "CPUStartDateTime"), CSV.replace(",10", ",90")]:
            with self.subTest(csv=csv), self.assertRaises(ValueError):
                module.summarize(TRACE, 7, csv)
        with self.assertRaises(ValueError):
            module.summarize(TRACE, 99)

    def test_realistic_qpc_frequency_and_display_window_boundary(self):
        trace = TRACE.replace("qpc_ticks=1000", "qpc_ticks=10000000").replace(
            "qpc_ticks=1100", "qpc_ticks=11000000"
        ).replace("qpc_frequency=1000", "qpc_frequency=10000000")
        csv = """ProcessID,SwapChainAddress,CPUStartQPC,DisplayLatency,DisplayedTime
7,0xA,9990000,2,10
7,0xA,10090000,2,10
7,0xA,10990000,2,10
"""
        result = module.summarize(trace, 7, csv)["presentation"]
        self.assertEqual(result["submitted_in_window"], 2)
        self.assertEqual(result["displayed_in_window"], 2)
        self.assertEqual(result["display_interval_ms"]["median"], 10)


if __name__ == "__main__":
    unittest.main()
