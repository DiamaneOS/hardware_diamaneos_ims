/* SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 The DiamaneOS Project
 */
package org.diamaneos.emergencylocation;

import org.w3c.dom.Element;
import org.w3c.dom.Node;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Properties;

import javax.xml.XMLConstants;
import javax.xml.parsers.DocumentBuilderFactory;

public final class LabMain {
    public static void main(String[] args) throws Exception {
        if (args.length < 1)
            throw new IllegalArgumentException("run, countries or validate-profiles required");
        switch (args[0]) {
            case "countries":
                for (String c : Locale.getISOCountries())
                    System.out.println(c.toLowerCase(Locale.ROOT));
                return;
            case "matrix":
                if (args.length != 3)
                    throw new IllegalArgumentException(
                            "matrix needs scenario directory and countries");
                String[] countries =
                        args[2].equals("all") ? Locale.getISOCountries() : args[2].split(",");
                if (countries.length > 676) throw new IllegalArgumentException("too many countries");
                boolean failed = false;
                try (var paths = Files.list(Path.of(args[1]))) {
                    List<Path> files =
                            paths.filter(p -> p.toString().endsWith(".properties"))
                                    .sorted()
                                    .toList();
                    if (files.isEmpty() || files.size() > 128)
                        throw new IllegalArgumentException("scenario count outside bounds");
                    for (String country : countries)
                        for (Path file : files) {
                            if (Files.size(file) > 65536)
                                throw new IllegalArgumentException("scenario exceeds 64 KiB");
                            Properties scenario = new Properties();
                            try (var reader = Files.newBufferedReader(file)) {
                                scenario.load(reader);
                            }
                            scenario.setProperty("country", country.toLowerCase(Locale.ROOT));
                            Map<String, String> result = AmlScenario.run(scenario);
                            failed |= !result.get("result").equals("PASS");
                            System.out.println(json(result));
                        }
                }
                if (failed) System.exit(1);
                return;
            case "validate-profiles":
                validateProfiles();
                return;
            case "run":
                Properties input = new Properties();
                byte[] data = System.in.readNBytes(65537);
                if (data.length > 65536) throw new IllegalArgumentException("scenario too large");
                input.load(new java.io.StringReader(new String(data, StandardCharsets.UTF_8)));
                Map<String, String> report = AmlScenario.run(input);
                System.out.println(json(report));
                if (report.get("result").equals("FAIL")) System.exit(1);
                return;
            default:
                throw new IllegalArgumentException("unknown operation");
        }
    }

    private static void validateProfiles() throws Exception {
        // Host input is untrusted XML. No DTD, entity expansion, XInclude or external access.
        byte[] xml = System.in.readNBytes(1048577);
        if (xml.length > 1048576) throw new IllegalArgumentException("profile XML too large");
        DocumentBuilderFactory factory = DocumentBuilderFactory.newInstance();
        factory.setFeature(XMLConstants.FEATURE_SECURE_PROCESSING, true);
        factory.setFeature("http://apache.org/xml/features/disallow-doctype-decl", true);
        factory.setFeature("http://xml.org/sax/features/external-general-entities", false);
        factory.setFeature("http://xml.org/sax/features/external-parameter-entities", false);
        factory.setXIncludeAware(false);
        factory.setExpandEntityReferences(false);
        factory.setAttribute(XMLConstants.ACCESS_EXTERNAL_DTD, "");
        factory.setAttribute(XMLConstants.ACCESS_EXTERNAL_SCHEMA, "");
        Element root =
                factory.newDocumentBuilder()
                        .parse(new java.io.ByteArrayInputStream(xml))
                        .getDocumentElement();
        if (!root.getTagName().equals("aml-profiles") || root.getAttributes().getLength() != 0)
            throw new IllegalArgumentException("wrong root");
        List<AmlProfile> profiles = new ArrayList<>();
        for (Node node = root.getFirstChild(); node != null; node = node.getNextSibling()) {
            if (node.getNodeType() != Node.ELEMENT_NODE) continue;
            Element e = (Element) node;
            if (!e.getTagName().equals("profile")
                    || profiles.size() >= AmlProfiles.MAX_PROFILES
                    || e.getElementsByTagName("*").getLength() != 0)
                throw new IllegalArgumentException("invalid profile structure");
            Map<String, String> attrs = new java.util.HashMap<>();
            for (int i = 0; i < e.getAttributes().getLength(); i++) {
                Node attr = e.getAttributes().item(i);
                attrs.put(attr.getNodeName(), attr.getNodeValue());
            }
            profiles.add(AmlProfiles.parse(attrs));
        }
        new AmlProfiles(profiles);
        long expired =
                profiles.stream().filter(p -> p.expiresUtcMs <= System.currentTimeMillis()).count();
        System.out.println(
                "{\"schema_valid\":true,\"profiles\":"
                        + profiles.size()
                        + ",\"expired\":"
                        + expired
                        + ",\"receiver_verified\":false}");
        if (expired > 0) System.exit(1);
    }

    static String json(Map<String, String> values) {
        List<String> fields = new ArrayList<>();
        values.forEach(
                (key, value) ->
                        fields.add(
                                quote(key)
                                        + ":"
                                        + (key.equals("real_world_validated")
                                                ? "false"
                                                : quote(value))));
        return "{" + String.join(",", fields) + "}";
    }

    private static String quote(String s) {
        StringBuilder b = new StringBuilder("\"");
        for (char c : s.toCharArray()) {
            if (c == '"' || c == '\\') b.append('\\').append(c);
            else if (c < 32) b.append(String.format(Locale.ROOT, "\\u%04x", (int) c));
            else b.append(c);
        }
        return b.append('"').toString();
    }
}
