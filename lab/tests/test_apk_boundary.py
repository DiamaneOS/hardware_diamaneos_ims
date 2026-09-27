# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]

class ApkBoundaryTest(unittest.TestCase):
    def test_standalone_manifest_has_no_privileged_or_radio_components(self):
        root = ET.parse(ROOT/'lab/android/AndroidManifest.xml').getroot()
        self.assertEqual(root.attrib['package'],'org.diamaneos.emergencylocation.lab')
        for tag in ['uses-permission','instrumentation','receiver','service','provider']:
            self.assertEqual(list(root.iter(tag)),[])
    def test_production_build_does_not_include_lab_sources(self):
        bp = (ROOT/'aml/Android.bp').read_text()
        self.assertNotIn('lab/',bp)
        self.assertNotIn('LabActivity',(ROOT/'aml/AndroidManifest.xml').read_text())

if __name__ == '__main__': unittest.main()
