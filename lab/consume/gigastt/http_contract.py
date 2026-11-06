"""Shared identities and exact comparisons for the pinned HTTP campaign."""

import hashlib
import math
from fractions import Fraction
from pathlib import Path

MODEL_FILES = {
    "v3_rnnt_encoder_int8.onnx": "1d5a6f580b692e38ba35ed98d0f54648808f27b0a147dc11bd8978c15dc1a18f",
    "v3_rnnt_decoder.onnx": "443c3b7bd42b453611618135d6b1e7d9467e5dd97c8a68501da4aa355750c0da",
    "v3_rnnt_joint.onnx": "fd1d02f45c2ad3d6b67cc149811ad794ab4b020ed49a0a9e2790a8619d1cddd8",
    "v3_vocab.txt": "a9143c30844d3c0bee3e9e927e4084774eb1b9eeaafc473b2c4521e4911a7c07",
}
MODEL_SHA = MODEL_FILES["v3_rnnt_encoder_int8.onnx"]
ORT_SHA = "0bb8a9982b44df690195c2c34b75ca791c3b9f20070b8cecbd8f50c6264dd2e2"
HUMAN_SHA = hashlib.sha256("ищи телеканал про бизнес на тиви".encode()).hexdigest()
BASELINE_G04_SHA = hashlib.sha256("ищи телеканал про бизнес на тв".encode()).hexdigest()
TOLERANCE = Fraction(1, 16000)
THRESHOLDS = {"max_latency_ratio": 1.5, "max_rss_ratio": 1.25, "rss_slack_mib": 32}
# Immutable input identities from the qualified 2026-11-05 campaign.
CORPUS_SHA256 = {
    "golos_00-wav": "500d2e88a1634ee766fec46f83cb903f3a1c276b2cedb36f11db800a1c4c14d0",
    "golos_00-aac": "a662b21e9eb8e5987a2c4e6fc87ed953e3d72e1b0a4e94a12fe93aff5126e870",
    "golos_01-wav": "63666d51b938f5c9acf4d92e9984251fff122d9425232c75ec45ee81cf7aab1c",
    "golos_01-aac": "87bf8aa989a2e35661649af67e9729980301aeb0e9b72868637d73e48e355758",
    "golos_02-wav": "101105b34eb3dbad83eb27849218c1b56756b2265f85d1922f32983876462e1f",
    "golos_02-aac": "bbae30dd1eafd2899db2fb9faa5c650a444693d7a0ec343df4d07dfeb236244a",
    "golos_03-wav": "d5e5002ab533245a61757ef267c75c298dfc7a22904fdb5385374ade2276a240",
    "golos_03-aac": "041043a8beddbce68a5b357b68e46920010a58a047b232f65f19a8e071cb75eb",
    "golos_04-wav": "606918e23e0894f7c330441d3e8e4eacb37374722f95fc0a73cf91515a4cc758",
    "golos_04-aac": "d681d1c01582c90765ccf186c2faed4daeb9ecc0aa020736539adb0c61189a6f",
    "golos_05-wav": "ed0f1cfe5a6d48694e54d8a3357901acbc0f1113dc846ee01cb00a405a112c04",
    "golos_05-aac": "c353c6b45afb5093d4303562cd671e99233456b573ad3d7b8a129f9eb95c2134",
    "golos_06-wav": "eb3042fedddbe16c5c5857b3ae9443d464a42a2d0bbc829a61714af5e7b240f8",
    "golos_06-aac": "d248805919d4dc8965d00195edb59bfd534824f68dcfad4e48dcbcece165cbe3",
    "golos_07-wav": "df9e4058f32ed3fe8cacdf9e31e92c7b6c2f49cd8adf5f50f853086ec4207867",
    "golos_07-aac": "f55166821815750878af9052af29fb4f70368521f6615e79f02fddf7dcfeface",
    "golos_08-wav": "23e2fcf0734d97acd6837bd85591ce998e385f5b32bd8a55ef217b689f00d8a7",
    "golos_08-aac": "671e862060e8febec6f76e9c2aa5213685879c42057898b9070737bdd7c88735",
    "golos_09-wav": "a23daa8fa6529d02a7e931fb507ac3be998de6c3e10475c693d4b7fc2f086b68",
    "golos_09-aac": "51b7b6d7c8d39a79356eec609180575749d4e4dbcbfd684987966544d31589ac",
    "golos_10-wav": "fd681926b2d8087d1772246d6c95110695f253190b0cef581d25c80977477d81",
    "golos_10-aac": "ee8b94dde3d282459849af00038043976989706011cfd5479a687fb247565eb3",
    "golos_11-wav": "98e3db631196538177d6e1fc1db34da93bd68e6e820b1a5b73df3bff59177f72",
    "golos_11-aac": "948602e6ce2fa9bd193d118204481d0ce72c2bcd1660cedf9393f24458bd7488",
    "golos_12-wav": "d9b6fea6d4b9cef70df3241d4280d5dcf0ceadc0a6bd2ca240e8ee30cbef5e4b",
    "golos_12-aac": "f88e12f972469a03a13a2364058668fb1ecf9334842dc485a81d2601b6405dca",
    "golos_13-wav": "a84b9e37c1474cc57a34a9a5e30215864c52c4e9e98271b634a42541163370c0",
    "golos_13-aac": "d08ec4308ec3e217b9bc387c4681296407caed9c6c48eb14be080ff29a30314d",
    "golos_14-wav": "5711ab2be5b07eb1c0c2af925f5586da5db3805cad0a0b7c1a8b84a03b894cb8",
    "golos_14-aac": "a064c203afd9aa8abc0a6d565ea92b76f1e94fbba88e7b9f10b37e26bf36bd0e",
    "golos_00-he1": "0ad243e2a85435b51de75e81567490fca460caca6e424691972b4241eded9fb8",
    "golos_00-he2": "f6a368393b086b38192590d854c5b260c46a775cc46433bc4a70ea6f5cc4fe1d"
}
CASE_IDS = set(CORPUS_SHA256)


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            value.update(chunk)
    return value.hexdigest()


def finite_number(value):
    return type(value) in (int, float) and (type(value) is int or math.isfinite(value))


def valid_digest(value):
    return (isinstance(value, str) and len(value) == 64
            and all(character in "0123456789abcdef" for character in value))


def close(a, b):
    if not finite_number(a) or not finite_number(b):
        return False
    # Recorded decimal durations have an inclusive one-sample bound. Binary
    # subtraction incorrectly rejected that boundary for golos_05 and _12.
    return abs(Fraction(str(a)) - Fraction(str(b))) <= TOLERANCE


def seconds(samples):
    return samples / 16000 if type(samples) is int and samples >= 0 else None


def index_cases(rows, label="corpus"):
    """Require each pinned case exactly once, before converting to a mapping."""
    if not isinstance(rows, list) or len(rows) != len(CASE_IDS):
        raise ValueError(f"{label}: expected exactly 32 distinct pinned corpus cases")
    if any(not isinstance(row, dict) or not isinstance(row.get("id"), str) for row in rows):
        raise ValueError(f"{label}: case identity is missing")
    indexed = {row["id"]: row for row in rows}
    if len(indexed) != len(rows) or set(indexed) != CASE_IDS:
        raise ValueError(f"{label}: expected exactly 32 distinct pinned corpus cases")
    return indexed
