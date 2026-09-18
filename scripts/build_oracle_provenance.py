#!/usr/bin/env python3
"""Write corpus/oracles/provenance.json for committed goldens. No remint."""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from corpus_synth import sha256_of  # noqa: E402


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def rec(**kw) -> dict:
    kw.setdefault("engine_build", None)
    kw.setdefault("priming_samples", None)
    kw.setdefault("remainder_samples", None)
    kw.setdefault("valid_samples", None)
    kw.setdefault("channel_order", None)
    kw.setdefault("command", None)
    kw.setdefault("settings", None)
    kw.setdefault("provenance_gap", None)
    return kw


def main() -> int:
    g = ROOT / "src/goldens"
    e = ROOT / "src/engine/goldens"
    records = [
        rec(
            id="enc48.lavc.s16",
            path="src/goldens/enc48.lavc.s16",
            role="lavc-pcm-oracle",
            engine="ffmpeg libavcodec (offline mint)",
            command="ffmpeg -y -i src/goldens/enc48.adts -f s16le src/goldens/enc48.lavc.s16",
            settings="s16le interleaved stereo 48 kHz; tests never spawn ffmpeg",
            pcm_precision="s16le",
            channel_order="L R",
            priming_samples=1024,
            comparison="deterministic",
            provenance_gap="re-minted 2026-09-15 with ffmpeg 7.0.2-static (johnvansickle) after TASK-113; earlier mint unrecorded",
        ),
        rec(
            id="enc48m.lavc.s16",
            path="src/goldens/enc48m.lavc.s16",
            role="lavc-pcm-oracle",
            engine="ffmpeg libavcodec (offline mint)",
            command="ffmpeg -y -i src/goldens/enc48m.m4a -f s16le src/goldens/enc48m.lavc.s16",
            pcm_precision="s16le",
            channel_order="L R",
            priming_samples=1024,
            comparison="deterministic",
            provenance_gap="re-minted 2026-09-15 with ffmpeg 7.0.2-static (johnvansickle) after TASK-113; earlier mint unrecorded",
        ),
        rec(
            id="enc48t.lavc.s16",
            path="src/goldens/enc48t.lavc.s16",
            role="lavc-pcm-oracle",
            engine="ffmpeg libavcodec (offline mint)",
            command="ffmpeg -y -i src/goldens/enc48t.adts -f s16le src/goldens/enc48t.lavc.s16",
            pcm_precision="s16le",
            channel_order="L R",
            priming_samples=1024,
            comparison="deterministic",
            provenance_gap="re-minted 2026-09-15 with ffmpeg 7.0.2-static (johnvansickle) after TASK-113; earlier mint unrecorded",
        ),
        rec(
            id="enc48l.lavc.s16",
            path="src/goldens/enc48l.lavc.s16",
            role="lavc-pcm-oracle",
            engine="ffmpeg libavcodec (offline mint)",
            command="ffmpeg -y -i src/goldens/enc48l.adts -f s16le src/goldens/enc48l.lavc.s16",
            pcm_precision="s16le",
            channel_order="L R",
            priming_samples=1024,
            comparison="deterministic",
            provenance_gap="re-minted 2026-09-15 with ffmpeg 7.0.2-static (johnvansickle) after TASK-113; earlier mint unrecorded",
        ),
        rec(
            id="he48e.lavc.s16",
            path="src/goldens/he48e.lavc.s16",
            role="lavc-pcm-oracle",
            engine="ffmpeg libavcodec 7.0.2-static (offline mint)",
            command="ffmpeg -y -i src/goldens/he48e.adts -f s16le src/goldens/he48e.lavc.s16",
            settings="HE-AAC v1 ADTS (implicit SBR); s16le interleaved stereo 48 kHz",
            pcm_precision="s16le",
            channel_order="L R",
            priming_samples=3018,
            comparison="deterministic",
            provenance_gap="ffmpeg 7.0.2-static (johnvansickle) build flags not recorded",
        ),
        rec(
            id="he48em.lavc.s16",
            path="src/goldens/he48em.lavc.s16",
            role="lavc-pcm-oracle",
            engine="ffmpeg libavcodec 7.0.2-static (offline mint)",
            command="ffmpeg -y -i src/goldens/he48em.m4a -f s16le src/goldens/he48em.lavc.s16",
            settings="HE-AAC v1 M4A (explicit two-rate ASC); ffmpeg applies the edit list (3018 skipped)",
            pcm_precision="s16le",
            channel_order="L R",
            priming_samples=3018,
            valid_samples=28_800,
            comparison="deterministic",
            provenance_gap="ffmpeg 7.0.2-static (johnvansickle) build flags not recorded",
        ),
        rec(
            id="enc48lt.lavc.s16",
            path="src/goldens/enc48lt.lavc.s16",
            role="lavc-pcm-oracle",
            engine="ffmpeg libavcodec 7.0.2-static (offline mint)",
            command="ffmpeg -y -i src/goldens/enc48lt.latm -f s16le src/goldens/enc48lt.lavc.s16",
            settings="LC LOAS/LATM (aac_latm); s16le interleaved stereo 48 kHz",
            pcm_precision="s16le",
            channel_order="L R",
            priming_samples=1024,
            comparison="deterministic",
            provenance_gap="ffmpeg 7.0.2-static (johnvansickle) build flags not recorded",
        ),
        rec(
            id="he48elt.lavc.s16",
            path="src/goldens/he48elt.lavc.s16",
            role="lavc-pcm-oracle",
            engine="ffmpeg libavcodec 7.0.2-static (offline mint)",
            command="ffmpeg -y -i src/goldens/he48elt.latm -f s16le src/goldens/he48elt.lavc.s16",
            settings="HE-AAC v1 LOAS/LATM (aac_latm, explicit ASC); s16le interleaved stereo 48 kHz",
            pcm_precision="s16le",
            channel_order="L R",
            priming_samples=3018,
            comparison="deterministic",
            provenance_gap="ffmpeg 7.0.2-static (johnvansickle) build flags not recorded",
        ),
        rec(
            id="pns48.s16",
            path="src/goldens/pns48.s16",
            role="lavc-pcm-oracle",
            engine="ffmpeg libavcodec (offline mint)",
            pcm_precision="s16le",
            channel_order="mono",
            comparison="pns_stochastic",
            provenance_gap="PNS RNG sequence is lavc-specific; do not require waveform identity with unrelated RNGs. ffmpeg version unrecorded.",
        ),
        rec(
            id="engine-sine.s16",
            path="src/engine/goldens/sine.s16",
            role="naive-imdct-oracle",
            engine="syom naive §4.6.11 (MINT_GOLDENS=1 mint_audible_goldens)",
            command="MINT_GOLDENS=1 cargo test --lib mint_audible_goldens",
            pcm_precision="s16le",
            channel_order="mono",
            valid_samples=1024,
            comparison="deterministic",
            provenance_gap=None,
        ),
        rec(
            id="engine-noise.s16",
            path="src/engine/goldens/noise.s16",
            role="naive-imdct-oracle",
            engine="syom naive §4.6.11 (MINT_GOLDENS=1 mint_audible_goldens)",
            command="MINT_GOLDENS=1 cargo test --lib mint_audible_goldens",
            pcm_precision="s16le",
            channel_order="mono",
            valid_samples=1024,
            comparison="deterministic",
            provenance_gap=None,
        ),
    ]
    # TASK-62 7.1 fixtures: minted 2026-09-15 with ffmpeg 7.0.2-static from a
    # scratch 8-tone WAV (FL 440, FR 880, FC 330, LFE 100, BL 550, BR 660,
    # SL 770, SR 220; 0.16 FS, 0.3 s, 48 kHz), `-guess_layout_max 0`.
    mc71_src = "python3 sine WAV (see src/decode_mc_tests.rs), 8 ch 48 kHz 0.3 s"
    for stem, layout, cfg in (
        ("mc71", "7.1", "channel_configuration 7 (SCE C, CPE FL/FR, CPE SL/SR, CPE BL/BR, LFE)"),
        ("mc71p", "7.1(wide)", "channel_configuration 0 + in-band PCE (front CPE0 SCE0, side SCE1, back CPE1 CPE2, no LFE)"),
    ):
        enc = (
            f"ffmpeg -guess_layout_max 0 -i mc71_src.wav -af aformat=channel_layouts={layout} "
            f"-c:a aac -b:a 256000 {stem}.m4a && ffmpeg -i {stem}.m4a -c:a copy -f adts {stem}.adts"
        )
        for ext in (".adts", ".m4a"):
            if not (g / f"{stem}{ext}").is_file():
                continue
            records.append(
                rec(
                    id=f"{stem}{ext}",
                    path=f"src/goldens/{stem}{ext}",
                    role="committed-fixture",
                    engine="ffmpeg libavcodec native aac encoder 7.0.2-static (offline mint)",
                    command=enc,
                    settings=f"{cfg}; source {mc71_src}",
                    pcm_precision="bitstream",
                    comparison="deterministic",
                    priming_samples=1024,
                    provenance_gap="ffmpeg 7.0.2-static (johnvansickle) build flags not recorded",
                )
            )
        records.append(
            rec(
                id=f"{stem}.s16",
                path=f"src/goldens/{stem}.s16",
                role="lavc-pcm-oracle",
                engine="ffmpeg libavcodec 7.0.2-static (offline mint)",
                command=f"ffmpeg -i {stem}.adts -f s16le -acodec pcm_s16le {stem}.s16",
                settings=f"{cfg}; s16le interleaved 8 ch 48 kHz, untrimmed (ADTS)",
                pcm_precision="s16le",
                channel_order="FL FR FC LFE BL BR SL SR (lavc; mc71p: PCE declaration order, same tones)",
                priming_samples=1024,
                comparison="deterministic",
                provenance_gap="ffmpeg 7.0.2-static (johnvansickle) build flags not recorded",
            )
        )
    # TASK-114 surround encoder goldens: syom-encoded, libavcodec-decoded.
    for stem, order in (("enc_mc51", "FL FR FC LFE BL BR"), ("enc_mc71", "FL FR FC LFE BL BR SL SR")):
        records.append(
            rec(
                id=f"{stem}.lavc.s16",
                path=f"src/goldens/{stem}.lavc.s16",
                role="lavc-pcm-oracle",
                engine="ffmpeg libavcodec 7.0.2-static (offline mint)",
                command=f"ffmpeg -y -i src/goldens/{stem}.adts -f s16le src/goldens/{stem}.lavc.s16",
                settings="syom McEncoder ADTS (MINT_GOLDENS=1 cargo test --lib mint_mc_goldens); s16le interleaved 48 kHz",
                pcm_precision="s16le",
                channel_order=order,
                priming_samples=1024,
                comparison="deterministic",
                provenance_gap="ffmpeg 7.0.2-static (johnvansickle) build flags not recorded",
            )
        )
    # Remaining committed files: inventory with gap, no invented backend version.
    known = {r["path"] for r in records}
    for folder, prefix in ((g, "src/goldens"), (e, "src/engine/goldens")):
        for p in sorted(folder.iterdir()):
            rel = f"{prefix}/{p.name}"
            if rel in known or not p.is_file():
                continue
            is_pns = "pns" in p.name
            records.append(
                rec(
                    id=p.name,
                    path=rel,
                    role="committed-fixture",
                    engine="unknown (historical checkout)",
                    pcm_precision="s16le" if p.suffix == ".s16" else "bitstream",
                    comparison="pns_stochastic" if is_pns else "deterministic",
                    provenance_gap="mint backend version, exact command, priming/remainder not recorded; not reminted",
                )
            )

    synth = {
        "fn": "impulse",
        "n": 1024,
        "at": 0,
        "ch": 1,
        "rate": 48000,
    }
    records.append(
        rec(
            id="synth-oracle-impulse-0",
            path=None,
            role="self-minted-pcm-oracle",
            engine="scripts/corpus_synth.py",
            command="python3 scripts/corpus_synth.py (generate impulse n=1024 at=0)",
            settings=synth,
            pcm_precision="f32le planar",
            channel_order="mono",
            valid_samples=1024,
            priming_samples=0,
            remainder_samples=0,
            comparison="deterministic",
            sha256_override=sha256_of(synth),
            provenance_gap=None,
        )
    )

    for r in records:
        if r.get("path"):
            r["sha256"] = sha256_file(ROOT / r["path"])
        elif "sha256_override" in r:
            r["sha256"] = r.pop("sha256_override")
        else:
            raise SystemExit(f"no hash for {r['id']}")

    man = {
        "version": 1,
        "title": "syom oracle provenance",
        "notes": (
            "Ordinary cargo test must not invoke ffmpeg/FDK. Historical lavc "
            "s16 goldens keep their committed bytes; missing engine versions "
            "are gaps, not guesses. PNS comparisons are statistical/tool, not "
            "cross-RNG waveform identity."
        ),
        "policies": {
            "deterministic": (
                "bitstream byte-exact and/or PCM vs committed s16: "
                "max abs <= 2 LSB, SNR >= 55 dB (typical 1 LSB / 70-80 dB)"
            ),
            "pns_stochastic": (
                "PNS/noise tools: energy, codebook and lavc-LCG agreement "
                "where claimed; do not force waveform identity across RNGs"
            ),
        },
        "records": records,
    }
    dest = ROOT / "corpus" / "oracles" / "provenance.json"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(man, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {dest} ({len(records)} records)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
