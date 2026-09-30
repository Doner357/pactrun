import tempfile
import unittest
from pathlib import Path
from tools.verify_release_catalog import compare_files


class CatalogFiles(unittest.TestCase):
    def test_tampering_missing_and_extra_files_are_rejected(self):
        parent = Path(__file__).resolve().parents[1] / 'target'
        parent.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            root, generated = Path(directory) / 'source', Path(directory) / 'generated'
            for base in (root, generated):
                (base / 'bucket').mkdir(parents=True)
                (base / 'bucket/pactrun-preview.json').write_text('{"version":"1.0.0-alpha.1"}\n', encoding='utf-8')
            compare_files(root, generated)
            file = root / 'bucket/pactrun-preview.json'
            original = file.read_bytes()
            file.write_text('{"version":"changed"}\n', encoding='utf-8')
            with self.assertRaises(ValueError): compare_files(root, generated)
            file.unlink()
            with self.assertRaises(ValueError): compare_files(root, generated)
            file.write_bytes(original)
            (root / 'bucket/unreviewed.json').write_text('{}', encoding='utf-8')
            with self.assertRaises(ValueError): compare_files(root, generated)


if __name__ == '__main__':
    unittest.main()
