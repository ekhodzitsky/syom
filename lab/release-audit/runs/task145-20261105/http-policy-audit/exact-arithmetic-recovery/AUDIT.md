# Exact one-sample arithmetic recovery

Read-only independent review: no blockers found. No builds, tests, model loads
or HTTP requests were executed by this audit.

Actual golos 05/12 differences are exactly one sample: 3.204−3.2039375 and
5.944−5.9439375. Binary subtraction yielded 0.00006250000000029843, producing
a false rejection. The prior static audit did not exercise these real boundary
values; its artifact and the failed reference remain preserved.

The correction uses Fraction(str(value)) and an inclusive Fraction(1, 16000).
It adds no epsilon and changes no latency/RSS/load/fault threshold. Exact type
and nonfinite guards remain. Reviewed tests cover both boundary directions,
nextafter immediately inside/outside, and invalid values. The owner's retained
17-test log passes; this audit did not rerun tests.

Canary and independent JSON SHA values still match attempt1. Full FFmpeg coverage,
byte-prefix checks and narrowly pinned 09/10 clock constraints are unchanged.
Presentation generation, canary/load runners and clock parser hashes are unchanged.
The new attempt2 reference qualifies with no errors, while original_canary_pass
remains false. This does not establish completion of the separate real load/fault run.

Exact reviewed source hashes, unchanged raw hashes, original errors and new
reference identity are recorded in audit.json; reviewed-policy.diff preserves
all production policy changes for review.
