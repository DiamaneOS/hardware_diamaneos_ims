# SPDX-License-Identifier: Apache-2.0
import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
COMMON = runpy.run_path(str(ROOT/'lab/run'))

class HarnessTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls): cls.classes = COMMON['compile_host']()
    def invoke(self, operation, data):
        return subprocess.run([COMMON['java_tool']('java'),'-cp',str(self.classes),
            'org.diamaneos.emergencylocation.LabMain',operation],input=data,
            stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=20)
    def test_production_endpoints_cannot_be_injected_into_scenarios(self):
        result = self.invoke('run',b'https_endpoint=https://example.org\n')
        self.assertNotEqual(result.returncode,0)
    def test_wrong_expected_result_is_a_failure(self):
        result = self.invoke('run',b'expected_decision=NO_PROFILE\n')
        self.assertEqual(result.returncode,1)
        self.assertEqual(json.loads(result.stdout)['result'],'FAIL')
        self.assertIs(json.loads(result.stdout)['real_world_validated'],False)
    def test_late_connection_never_writes_stale_location(self):
        result = self.invoke('run',b'https=late_connect\nexpected_https=FAILED\n')
        self.assertEqual(result.returncode,0)
        self.assertEqual(json.loads(result.stdout)['https_bytes'],'0')
    def test_xml_entities_and_unknown_attributes_are_rejected(self):
        for data in [b'<!DOCTYPE aml-profiles [<!ENTITY x SYSTEM "file:///etc/passwd">]><aml-profiles>&x;</aml-profiles>',
                     b'<aml-profiles><profile surprise="1"/></aml-profiles>']:
            self.assertNotEqual(self.invoke('validate-profiles',data).returncode,0)
    def test_empty_profiles_are_not_claimed_verified(self):
        result = self.invoke('validate-profiles',b'<aml-profiles/>')
        report = json.loads(result.stdout)
        self.assertEqual(report['profiles'],0)
        self.assertFalse(report['receiver_verified'])

if __name__=='__main__': unittest.main()
