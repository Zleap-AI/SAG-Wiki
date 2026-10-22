"""Uses the actual docling-core model; no inference or model downloads."""
from pathlib import Path
import runpy
import unittest
from docling_core.types.doc import DoclingDocument, DocItemLabel, TrackSource

validate = runpy.run_path(str(Path(__file__).with_name("docling_media_runner.py")))["validate_probe_document"]


class TimestampProbeTests(unittest.TestCase):
    def test_actual_docling_track_source_list_is_accepted(self):
        document = DoclingDocument(name="probe")
        item = document.add_text(label=DocItemLabel.TEXT, text="The quick brown fox.",
                                 source=TrackSource(start_time=0, end_time=2.5))
        self.assertIsInstance(item.source, list)
        self.assertIsInstance(item.source[0], TrackSource)
        validate(document, "the quick brown fox")

    def test_recognized_speech_without_track_timestamps_is_rejected(self):
        document = DoclingDocument(name="probe")
        document.add_text(label=DocItemLabel.TEXT, text="The quick brown fox.")
        with self.assertRaisesRegex(RuntimeError, "provenance"):
            validate(document, "the quick brown fox")

    def test_a_track_alone_does_not_satisfy_video_frame_verification(self):
        document = DoclingDocument(name="probe")
        document.add_text(label=DocItemLabel.TEXT, text="The quick brown fox.",
                          source=TrackSource(start_time=0, end_time=2.5))
        with self.assertRaisesRegex(RuntimeError, "sampled frame"):
            validate(document, "the quick brown fox", video_probe=True)


if __name__ == "__main__":
    unittest.main()
