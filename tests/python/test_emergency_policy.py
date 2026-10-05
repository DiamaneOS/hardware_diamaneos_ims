# SPDX-License-Identifier: Apache-2.0
"""Pin the emergency service guarantees that live in build, policy and init
files rather than in testable code: user builds always serve the emergency
PDN, the kill switches only work on debuggable builds, nothing marks the
daemon critical and the broker cannot be disabled. Reads files only."""
from pathlib import Path
import re
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
KILLS = ('vendor.diamaneos.ims.emergency_pdn_kill', 'vendor.diamaneos.ims.dcm_kill')


def text(path):
    return (ROOT / path).read_text()


def block(source, opener):
    """The brace-delimited body that starts at the unique opener line."""
    assert source.count(opener) == 1, opener
    start = source.index(opener) + len(opener)
    depth = 1
    for i in range(start, len(source)):
        depth += {'{': 1, '}': -1}.get(source[i], 0)
        if depth == 0:
            return source[start:i]
    raise AssertionError('unbalanced block')


class EmergencyPolicyTest(unittest.TestCase):
    def test_product_serves_emergency_and_ships_the_switches_off(self):
        props = dict(re.findall(r'^\s*([a-z][\w.]+)=(\S+)', text('ims-product.mk'), re.M))
        self.assertEqual(props['ro.vendor.diamaneos.ims.emergency_pdn'], 'serve')
        for name in KILLS:
            self.assertEqual(props[name], '0')

    def test_switches_are_vendor_internal_and_only_build_defaults_set_them(self):
        contexts = text('sepolicy/vendor/property_contexts')
        for name in KILLS:
            self.assertRegex(contexts, rf'(?m)^{re.escape(name)}\s+'
                             r'u:object_r:vendor_diamaneos_imsdcm_prop:s0 exact enum 0 1$')
        self.assertIn('vendor_internal_prop(vendor_diamaneos_imsdcm_prop)',
                      text('sepolicy/vendor/property.te'))
        setters = [p.relative_to(ROOT).as_posix() for p in (ROOT / 'sepolicy').rglob('*.te')
                   if re.search(r'set_prop\([^)]*vendor_diamaneos_imsdcm_prop', p.read_text())]
        self.assertEqual(setters, ['sepolicy/vendor/vendor_init.te'])
        self.assertIn('neverallow diamaneos_imsdcm property_type:property_service set;',
                      text('sepolicy/vendor/imsdcm.te'))

    def test_daemon_honours_the_switches_only_on_debuggable_builds(self):
        main = text('daemon/src/main.rs')
        self.assertIn('let debug_controls = prop("ro.debuggable").as_deref() == Some("1");', main)
        self.assertIn('let mut emergency_enabled = true;', main)
        gated = block(main, 'if debug_controls && now >= next_kill_check {')
        # Every live read of a switch, and the only change of the emergency
        # state, sit inside the debuggable-only block.
        for read in ('prop(EMERGENCY_PDN_KILL)', 'prop(DCM_KILL)'):
            self.assertEqual(main.count(read), 1, read)
            self.assertIn(read, gated)
        assignments = re.findall(r'(?<![\w.])emergency_enabled\s*=[^=]', main)
        self.assertEqual(len(assignments), 2)  # The `let` above and the gated read.
        self.assertEqual(len(re.findall(r'(?<![\w.])emergency_enabled\s*=[^=]', gated)), 1)
        self.assertIn('engine.set_emergency_enabled(emergency_enabled)', main)
        self.assertRegex(main, r'prop\("ro\.vendor\.diamaneos\.ims\.emergency_pdn"\)\.as_deref\(\)'
                               r' != Some\("serve"\) \{\s*return Err')

    def test_engine_starts_serving_emergency(self):
        new = block(text('dcm/src/engine.rs'), 'pub fn new(modem_node: u32, slots: u32)'
                    ' -> Result<Self, &\'static str> {')
        self.assertIn('emergency_enabled: true,', new)

    def test_init_starts_the_daemon_on_every_build_and_stops_it_only_when_debuggable(self):
        control = text('vintf/imsdcmd-control.rc')
        sections = re.split(r'(?m)^(?=on )', control)
        starts = [s for s in sections if 'start vendor.imsdcmd' in s]
        self.assertTrue(any(s.startswith('on property:ro.persistent_properties.ready=true\n')
                            for s in starts))
        for section in sections:
            if 'stop vendor.imsdcmd' in section:
                self.assertTrue(section.startswith('on property:ro.debuggable=1 && '), section)
        service = text('vintf/imsdcmd.rc').split('service vendor.imsdcmd ', 1)[1]
        options = {line.split()[0] for line in service.splitlines()[1:]
                   if line.strip() and not line.strip().startswith('#')}
        self.assertIn('disabled', options)
        # A crash must never reboot the phone; init restarts the daemon.
        self.assertFalse(options & {'critical', 'oneshot'}, options)

    def test_broker_runs_before_unlock_and_cannot_be_disabled(self):
        android = '{http://schemas.android.com/apk/res/android}'
        app = ET.parse(ROOT / 'broker/AndroidManifest.xml').getroot().find('application')
        self.assertEqual(app.get(android + 'persistent'), 'true')
        self.assertEqual(app.get(android + 'directBootAware'), 'true')
        config = ET.parse(ROOT / 'permissions/sysconfig-de.diamaneos.imsbroker.xml').getroot()
        self.assertEqual([e.get('package') for e in config.iter('prevent-disable')],
                         ['de.diamaneos.imsbroker'])


if __name__ == '__main__':
    unittest.main()
