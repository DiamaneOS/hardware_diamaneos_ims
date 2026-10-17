# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import unittest
import xml.etree.ElementTree as ET
spec=importlib.util.spec_from_file_location('apns',Path(__file__).resolve().parents[2]/'integration/merge_emergency_apns.py')
apns=importlib.util.module_from_spec(spec);spec.loader.exec_module(apns)
class ApnsTest(unittest.TestCase):
    def test_merges_emergency_only_preserving_attributes_and_is_idempotent(self):
        base=ET.fromstring('<apns version="8"><apn carrier="base" mcc="262" mnc="01" apn="internet" type="default"/></apns>')
        stock=ET.fromstring('<apns version="8"><apn carrier="test" mcc="262" mnc="01" apn="sos" type="emergency" protocol="IPV4V6"/><apn carrier="extra" mcc="262" mnc="02" apn="other" type="default"/></apns>')
        out,n=apns.merge(base,stock);self.assertEqual(n,1);self.assertEqual(len(out),2)
        self.assertEqual(out[1].attrib,stock[0].attrib);self.assertEqual(len(base),1)
        self.assertEqual(apns.merge(out,stock)[1],0)
    def test_rejects_invalid_country_code(self):
        with self.assertRaises(ValueError):apns.merge(ET.fromstring('<apns/>'),ET.fromstring('<apns><apn mcc="bad" mnc="01" type="emergency"/></apns>'))
if __name__=='__main__':unittest.main()
