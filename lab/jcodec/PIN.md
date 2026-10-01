# JCodec pin (TASK-19)

Peer: **JCodec** pure-Java media framework, AAC decode lane (JAAD-derived
decoder). Contextual cross-runtime lane only — never a syom product
dependency. License: FreeBSD (JCodec) / Public Domain (embedded JAAD,
`net.sourceforge.jaad`).

## Maven artifact (primary pin)

`org.jcodec:jcodec:0.2.5` — latest and release version on Maven Central;
`maven-metadata.xml` `lastUpdated` 2019-06-16 (no release since).

| File | SHA-256 | Maven SHA-1 (verified 2026-09-22) |
|---|---|---|
| jcodec-0.2.5.pom | `30f132c786d253a720acbc773fa800a4dfb23c8eb9bf7690bbbcc6aa33d4ee8a` | `f48d001cc27ad9319d91fee5638732ec9214108b` |
| jcodec-0.2.5.jar | `890329dad124e8b739c1d6602a59a53c8a474daddff265c2561e21c498496c81` | `0b1968a3dbf46aa5ac9630ce9e3cbf67c4940d08` |
| jcodec-0.2.5-sources.jar | `9945efa92e16b3fbe8eb8bf37f00be53fa0adb52d09ac978a0744ca2ef3697db` | `4f513ac8e20a4c10e4f90c3d46e34acae37e3c33` |

Do not vendor the jars in this crate. Fetch offline:

```sh
curl -fL -O https://repo1.maven.org/maven2/org/jcodec/jcodec/0.2.5/jcodec-0.2.5.jar
echo 890329dad124e8b739c1d6602a59a53c8a474daddff265c2561e21c498496c81  jcodec-0.2.5.jar | sha256sum -c
curl -fL -O https://repo1.maven.org/maven2/org/jcodec/jcodec/0.2.5/jcodec-0.2.5-sources.jar
echo 9945efa92e16b3fbe8eb8bf37f00be53fa0adb52d09ac978a0744ca2ef3697db  jcodec-0.2.5-sources.jar | sha256sum -c
```

## Source revision

GitHub `jcodec/jcodec` HEAD observed 2026-09-22:
`4ba922aca445c4df59f99e183c79d0538e5cb53f` (2025-05-14, "Lenient Parsing of
SL", fixes #517) — matches the doc-2 competitor-ledger entry. **There is no
v0.2.4/v0.2.5 git tag** (newest tag is v0.2.3); the 0.2.5 artifact predates
it. Nearest commit before the 2019-06-16 publish:
`05b2e96dc7970fff12b7226ed057d77c39b6b7ae` (2019-05-08). The pinned binary
artifact above, not a git checkout, is the immutable reference; capability
claims in REPORT.md were read from `jcodec-0.2.5-sources.jar`.

## Runtime

2026-09-22 had no JDK. 2026-09-28 ran the lane with a user-local
Temurin 21.0.12.1+1 (the system image still has no `java` on the
default PATH). Numbers are in REPORT.md.
