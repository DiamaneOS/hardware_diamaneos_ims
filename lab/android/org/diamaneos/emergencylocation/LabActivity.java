/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.Locale;
import java.util.Map;
import java.util.Properties;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Standalone simulator. No phone permission or production component is present in this APK. */
public final class LabActivity extends Activity {
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private TextView output;
    private EditText country;
    private Button run, load;

    @Override
    public void onCreate(Bundle state) {
        super.onCreate(state);
        LinearLayout content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        int pad = (int) (16 * getResources().getDisplayMetrics().density);
        content.setPadding(pad, pad, pad, pad);
        TextView intro = new TextView(this);
        intro.setText(
                "AML Lab\n\n"
                    + "Synthetic tests only. No calls, SMS, network requests, location access or"
                    + " device identifiers. Results do not establish emergency-service coverage.");
        content.addView(intro);
        country = new EditText(this);
        country.setSingleLine(true);
        country.setHint("ISO country code (for example de), or all");
        country.setText("de");
        country.setContentDescription("Country code for synthetic profiles");
        content.addView(country);
        run = new Button(this);
        run.setText("Run bundled scenarios");
        content.addView(run);
        load = new Button(this);
        load.setText("Open scenario file");
        content.addView(load);
        output = new TextView(this);
        output.setTextIsSelectable(true);
        content.addView(output);
        ScrollView scroll = new ScrollView(this);
        scroll.addView(content);
        setContentView(scroll);
        run.setOnClickListener(v -> runSuite());
        load.setOnClickListener(
                v ->
                        startActivityForResult(
                                new Intent(Intent.ACTION_OPEN_DOCUMENT)
                                        .setType("*/*")
                                        .addCategory(Intent.CATEGORY_OPENABLE),
                                1));
    }

    private void busy(boolean value) {
        run.setEnabled(!value);
        load.setEnabled(!value);
    }

    private void show(String text) {
        runOnUiThread(
                () -> {
                    if (!isDestroyed()) {
                        output.setText(text);
                        busy(false);
                    }
                });
    }

    private void runSuite() {
        String selected = country.getText().toString().trim().toLowerCase(Locale.ROOT);
        if (!selected.equals("all") && !selected.matches("[a-z]{2}")) {
            output.setText("Enter a two-letter code or all.");
            return;
        }
        busy(true);
        output.setText("Running synthetic cases…");
        worker.execute(
                () -> {
                    try {
                        String[] countries =
                                selected.equals("all")
                                        ? Locale.getISOCountries()
                                        : new String[] {selected};
                        String[] files = getAssets().list("");
                        Arrays.sort(files);
                        int count = 0, passed = 0;
                        StringBuilder details = new StringBuilder();
                        for (String code : countries)
                            for (String file : files) {
                                if (Thread.currentThread().isInterrupted()) return;
                                if (!file.endsWith(".properties")) continue;
                                Properties p = new Properties();
                                try (InputStream stream = getAssets().open(file)) {
                                    p.load(new InputStreamReader(stream, StandardCharsets.UTF_8));
                                }
                                p.setProperty("country", code.toLowerCase(Locale.ROOT));
                                Map<String, String> result = AmlScenario.run(p);
                                count++;
                                boolean pass = result.get("result").equals("PASS");
                                if (pass) passed++;
                                if (countries.length == 1 || !pass)
                                    details.append(file)
                                            .append(": ")
                                            .append(result.get("result"))
                                            .append('\n');
                            }
                        show(
                                passed
                                        + "/"
                                        + count
                                        + " synthetic cases passed across "
                                        + countries.length
                                        + " country profiles.\n\n"
                                        + details
                                        + "\nNo native IMS or real-world delivery was tested.");
                    } catch (Exception e) {
                        show("Scenario validation failed. Check the input format and bounds.");
                    }
                });
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != 1 || result != RESULT_OK || data == null || data.getData() == null) return;
        busy(true);
        worker.execute(
                () -> {
                    try (InputStream stream =
                            getContentResolver().openInputStream(data.getData())) {
                        if (stream == null) throw new IllegalArgumentException();
                        byte[] bytes = stream.readNBytes(65537);
                        if (bytes.length > 65536) throw new IllegalArgumentException();
                        Properties p = new Properties();
                        p.load(new java.io.StringReader(new String(bytes, StandardCharsets.UTF_8)));
                        Map<String, String> report = AmlScenario.run(p);
                        StringBuilder summary = new StringBuilder();
                        report.forEach(
                                (key, value) ->
                                        summary.append(key)
                                                .append(": ")
                                                .append(value)
                                                .append('\n'));
                        show(summary.toString());
                    } catch (Exception e) {
                        show(
                                "Invalid or unreadable scenario. No action was sent to a phone or"
                                    + " server.");
                    }
                });
    }

    @Override
    public void onDestroy() {
        worker.shutdownNow();
        super.onDestroy();
    }
}
