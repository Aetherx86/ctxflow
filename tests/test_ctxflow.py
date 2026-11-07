"""Offline tests (echo backend). Run: python3 -m unittest discover -s tests"""
import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
import re
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from ctxflow import advisor, backends, config, council, ledger  # noqa: E402
from ctxflow.cli import main  # noqa: E402
from ctxflow.engine import Engine  # noqa: E402
from ctxflow.state import Project, Turn  # noqa: E402
from ctxflow.tokens import estimate_tokens, prune_for_history  # noqa: E402

ECHO_CFG = """
[routing]
light = ["echo:light"]
summary = ["echo:sum"]
heavy = ["echo:heavy"]
[context]
keep_turns = 2
fold_when_turns = 4
[budget.models."echo:limited"]
rpd = 1
"""


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        os.environ["CTXFLOW_HOME"] = str(self.root / "_home")
        self.proj = Project(self.root)
        self.proj.init()
        (self.root / ".ctxflow" / "config.toml").write_text(ECHO_CFG)
        self.eng = Engine(self.proj, config.load(self.root))

    def tearDown(self):
        self.tmp.cleanup()

    def run_cli(self, *argv):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            main(["-C", str(self.root), *argv])
        return out.getvalue(), err.getvalue()


class TestTokens(unittest.TestCase):
    def test_estimate_hangul_denser(self):
        self.assertGreater(estimate_tokens("가" * 100), estimate_tokens("a" * 100))

    def test_prune_long_codeblock(self):
        code = "```py\n" + "\n".join(f"x{i} = {i}" for i in range(50)) + "\n```"
        pruned, stubs = prune_for_history("fix this\n" + code, 12, 6)
        self.assertIn("code:py 50 lines", pruned)
        self.assertLess(len(pruned), len(code) / 3)
        self.assertEqual(stubs[0]["body"], code)

    def test_prune_keeps_short_codeblock(self):
        code = "```py\na = 1\n```"
        self.assertEqual(prune_for_history(code, 12, 6), (code, []))

    def test_prune_traceback(self):
        tb = "Traceback (most recent call last)\n" + "\n".join(
            f'  File "a.py", line {i}, in f' for i in range(20)) + "\nValueError: boom"
        pruned, stubs = prune_for_history(tb, 12, 6)
        self.assertIn("log 21 lines", pruned)
        self.assertEqual(len(stubs), 1)
        self.assertIn("ValueError: boom", pruned)


class TestWorkflow(Base):
    def test_prompt_contains_state_docs_and_request(self):
        s = self.proj.load_session()
        p = self.eng.build_prompt(s, "hello", [])
        self.assertIn("=== SPEC.md ===", p)
        self.assertIn("=== PROGRESS.md ===", p)
        self.assertTrue(p.endswith("=== CURRENT REQUEST ===\nhello"))

    def test_routing_classification(self):
        self.assertEqual(self.eng.ask("이 버그 원인 디버깅 해줘").task, "heavy")
        self.assertEqual(self.eng.ask("변수명 바꿔줘").task, "light")

    def test_attach_limit(self):
        a, b = self.root / "a.py", self.root / "b.py"
        a.write_text("a=1"), b.write_text("b=2")
        with self.assertRaises(SystemExit):
            self.eng.ask("x", [str(a), str(b)])
        self.eng.ask("x", [str(a), str(b)], force=True)

    def test_attachment_is_one_shot(self):
        f = self.root / "big.py"
        f.write_text("\n".join(f"line_{i} = {i}" for i in range(200)))
        self.eng.ask("refactor", [str(f)])
        s = self.proj.load_session()
        p = self.eng.build_prompt(s, "next", [])
        self.assertNotIn("line_150", p)
        self.assertIn("(attached: ", p)

    def test_rolling_fold(self):
        for i in range(4):
            self.eng.ask(f"q{i}")
        s = self.proj.load_session()
        self.assertEqual(len(s.turns), 2)
        self.assertIn("[echo] summary", s.summary)

    def test_progress_delta_collected_and_merged(self):
        from ctxflow import backends
        orig = backends._echo

        def fake(model, system, prompt, cfg, max_tokens=None):
            r = orig(model, system, prompt, cfg)
            if "CURRENT REQUEST" in prompt:
                r.text = "done.\n\n## PROGRESS DELTA\n- login API 완료\n- 다음: 토큰 갱신"
            return r
        backends._echo = fake
        try:
            self.eng.ask("implement login")
        finally:
            backends._echo = orig
        s = self.proj.load_session()
        self.assertEqual(s.pending_progress, ["- login API 완료", "- 다음: 토큰 갱신"])
        self.assertNotIn("PROGRESS DELTA", s.turns[-1].assistant)
        self.eng.update_progress(s)
        self.assertIn("[echo] updated", self.proj.read_doc("PROGRESS.md"))
        self.assertTrue(any((self.root / ".ctxflow" / "backups").iterdir()))

    def test_handoff_resets_session(self):
        self.eng.ask("a")
        self.eng.ask("b")
        text, archived = self.eng.handoff("note")
        self.assertTrue(archived.exists())
        self.assertIn("HANDOFF", (self.root / "HANDOFF.md").read_text())
        s = self.proj.load_session()
        self.assertEqual(s.turns, [])
        self.assertIn("[echo] summary", s.summary)

    def test_budget_fallback(self):
        cfg = self.eng.cfg
        cfg["routing"]["light"] = ["echo:limited", "echo:backup"]
        self.assertEqual(self.eng.ask("one").result.model, "echo:limited")
        self.assertEqual(self.eng.ask("two").result.model, "echo:backup")
        stats = ledger.usage_window()
        self.assertEqual(stats["echo:limited"]["rpd"], 1)

    def test_unknown_backend_falls_through(self):
        self.eng.cfg["routing"]["light"] = ["nope:x", "echo:ok"]
        self.assertEqual(self.eng.ask("hi").result.model, "echo:ok")


class TestCLI(Base):
    def test_cli_roundtrip(self):
        out, _ = self.run_cli("ask", "hello", "world")
        self.assertIn("[echo:light]", out)
        out, _ = self.run_cli("status")
        self.assertIn("1 turn(s) kept", out)
        out, _ = self.run_cli("prompt", "next step")
        self.assertIn("=== RECENT TURNS ===", out)
        out, _ = self.run_cli("handoff")
        self.assertIn("# HANDOFF", out)
        out, _ = self.run_cli("new", "--blank")
        s = json.loads((self.root / ".ctxflow" / "session.json").read_text())
        self.assertEqual(s["summary"], "")



CARD = '{{"stance":"{s}","answer":"Use {s}.","confidence":{c},"reasons":["r"],"risks":["x"],"would_change":"load"}}'


def fake_backend(stances, reviews=None, calls=None):
    """Stands in for every provider. Seats answer by model name; reviewers score the text they see."""
    def call(spec, system, prompt, cfg, max_tokens=None):
        if calls is not None:
            calls.append((spec, system, prompt))
        if "Score the anonymous" in system:
            labels = re.findall(r"^([A-Z]): (.*)$", prompt, re.MULTILINE)
            scores = {l: {k: (reviews or (lambda a: 3))(a) for k in ("evidence", "risk", "feasibility")} for l, a in labels}
            text = json.dumps({"scores": scores})
        elif "final decision" in system:
            text = "**Decision:** chair"
        else:
            text = CARD.format(s=stances[spec], c=90)
        return backends.Result(text=text, model=spec, input_tokens=100, output_tokens=20, total_tokens=120)
    return call


class TestCouncil(Base):
    def setUp(self):
        super().setUp()
        with (self.root / ".ctxflow" / "config.toml").open("a") as fh:
            fh.write('[profiles.t]\nseats=["a:1","b:2","c:3"]\nchair="z:9"\nfallbacks=[]\nrouting={light=["echo:l"]}\n')
        (self.root / ".ctxflow" / "profile").write_text("t")
        self.eng = Engine(self.proj, config.load(self.root))

    def run_council(self, fake, **kw):
        with mock.patch.object(backends, "call", fake):
            return council.run(self.eng, "Which DB?", **kw)

    def test_agreement_makes_no_review_calls(self):
        calls = []
        r = self.run_council(fake_backend({"a:1": "pg", "b:2": "pg", "c:3": "pg"}, calls=calls))
        self.assertEqual(r["mode"], "agreed")
        self.assertEqual(len(calls), 3)

    def test_split_goes_to_matrix_then_stops_on_clear_winner(self):
        calls = []
        best = lambda a: 5 if "two" in a else 2  # noqa: E731
        r = self.run_council(fake_backend({"a:1": "one", "b:2": "two", "c:3": "three"}, best, calls))
        self.assertEqual(r["mode"], "ranked")
        self.assertIn("Use two.", r["final"])
        self.assertEqual(len(r["matrix"]), 3)
        self.assertEqual(len(calls), 6)
        self.assertFalse(any("z:9" == c[0] for c in calls), "no chair call")

    def test_tie_goes_to_chair(self):
        r = self.run_council(fake_backend({"a:1": "one", "b:2": "two", "c:3": "three"}))
        self.assertEqual(r["mode"], "resolved")
        self.assertEqual(r["final"], "**Decision:** chair")

    def test_model_failure_uses_fallback_and_is_reported(self):
        base = fake_backend({"a:1": "x", "b:2": "x", "c:3": "x", "f:1": "x"})

        def flaky(spec, *a, **k):
            if spec == "a:1":
                raise backends.BackendError("429", rate_limited=True)
            return base(spec, *a, **k)
        call = {"id": "s0", "model": "a:1", "fallbacks": ["f:1"], "system": "", "prompt": "q", "max_tokens": 50, "kind": "card"}
        with mock.patch.object(backends, "call", flaky):
            res = council._execute(self.eng, call)
        self.assertEqual(res["model"], "f:1")

    def test_daily_limit_blocks_before_any_call(self):
        ledger.record("gemini:x", 300000, 0, kind="t", estimated=False, project="p")
        calls = []
        with self.assertRaises(SystemExit):
            self.run_council(fake_backend({"a:1": "x"}, calls=calls))
        self.assertEqual(calls, [])

    def test_local_tokens_do_not_count_against_daily_limit(self):
        ledger.record("ollama:x", 900000, 0, kind="t", estimated=False, project="p")
        self.assertEqual(ledger.total_today(), 0)

    def test_mode_dial(self):
        self.run_cli("mode", "fast")
        self.eng = Engine(self.proj, config.load(self.root))
        calls = []
        r = self.run_council(fake_backend({"a:1": "one", "b:2": "two", "c:3": "three"}, calls=calls))
        self.assertEqual(r["mode"], "fast")
        self.assertEqual(len(calls), 3)
        self.run_cli("mode", "thorough")
        self.eng = Engine(self.proj, config.load(self.root))
        calls = []
        r = self.run_council(fake_backend({"a:1": "x", "b:2": "x", "c:3": "x"}, calls=calls))
        self.assertEqual(r["mode"], "resolved")
        self.assertEqual(len(calls), 7, "3 answers, 3 scorers, 1 final")

    def test_escalation_follows_the_dial(self):
        def fake(spec, system, prompt, cfg, max_tokens=None):
            text = "ESCALATE: pg or sqlite?" if "consulted only at checkpoints" in system else CARD.format(s="pg", c=90)
            return backends.Result(text=text, model=spec, input_tokens=10, output_tokens=5, total_tokens=15)
        for mode, expect_council in (("fast", False), ("auto", True)):
            self.run_cli("mode", mode)
            s = self.proj.load_session()
            s.advisor = {}
            self.proj.save_session(s)
            with mock.patch.object(backends, "call", fake):
                out, _ = self.run_cli("advise", "manual", "pick one")
            self.assertEqual("Use pg." in out, expect_council, mode)

    def test_cli_council_json(self):
        with mock.patch.object(backends, "call", fake_backend({"a:1": "pg", "b:2": "pg", "c:3": "pg"})):
            out, err = self.run_cli("council", "--yes", "--json", "Which DB?")
        self.assertEqual(json.loads(out)["mode"], "agreed")
        self.assertEqual(self.proj.load_session().spent_tokens, 360)


class TestAdvisor(Base):
    def consult(self, *a, reply="OK: fine", **k):
        seen = []

        def fake(spec, system, prompt, cfg, max_tokens=None):
            seen.append(prompt)
            return backends.Result(text=reply if len(seen) == 1 else "OK: restored", model=spec,
                                   input_tokens=10, output_tokens=5, total_tokens=15)
        with mock.patch.object(backends, "call", fake):
            return advisor.advise(self.eng, *a, **k), seen

    def test_plan_once_error_twice_then_blocked(self):
        a, seen = self.consult("plan", "plan?")
        self.assertTrue(a.consulted)
        self.assertEqual(len(seen), 1)
        self.assertFalse(self.consult("plan", "again")[0].consulted)
        err = "Traceback\nValueError: bad 12"
        self.assertFalse(self.consult("error", "why", error=err)[0].consulted)
        self.assertTrue(self.consult("error", "why", error=err.replace("12", "99"))[0].consulted)
        self.assertFalse(self.consult("error", "why", error=err)[0].consulted)
        self.assertTrue(self.consult("manual", "q")[0].consulted)
        self.assertFalse(self.consult("manual", "q")[0].consulted, "max_calls=3 reached")

    def test_brief_is_capped_and_never_the_transcript(self):
        s = self.proj.load_session()
        s.turns = [Turn(user="SECRETWORD " * 3000, assistant="ok", model="m") for _ in range(3)]
        self.proj.save_session(s)
        a, seen = self.consult("plan", "next?")
        self.assertLess(estimate_tokens(seen[0]), self.eng.cfg["advisor"]["brief_tokens"] + 50)
        self.assertIn("RECENT", a.dropped)

    def test_advisor_can_pull_one_original_by_id(self):
        self.proj.archive_put([{"id": "0123456789ab", "body": "ORIGINAL BYTES"}])
        a, seen = self.consult("manual", "q", reply="NEED: #0123456789ab")
        self.assertEqual(a.restored, ["0123456789ab"])
        self.assertIn("ORIGINAL BYTES", seen[1])
        self.assertEqual(len(seen), 2)


class TestMinimalLoss(Base):
    def test_pruned_original_restores_byte_for_byte(self):
        code = "```py\n" + "\n".join(f"v{i} = {i}" for i in range(40)) + "\n```"
        self.eng.ask("explain\n" + code)
        stored = self.proj.load_session().turns[0].user
        ref = re.search(r"#([0-9a-f]{12})", stored).group(1)
        self.assertEqual(self.run_cli("show", ref)[0], code)
        prompt = self.eng.build_prompt(self.proj.load_session(), "again", [], refs=[ref])
        self.assertIn("v39 = 39", prompt)

    def test_summary_cannot_lose_identifiers(self):
        self.eng.ask("TypeError in src/auth/session.py at refresh_token() on v1.82.6")
        self.eng.fold_now()
        summary = self.proj.load_session().summary
        for ref in ("src/auth/session.py", "refresh_token", "1.82.6"):
            self.assertIn(ref, summary)

    def test_secrets_never_reach_cloud_backend(self):
        seen = []

        def spy(model, system, prompt, cfg, max_tokens=None):
            seen.append(prompt)
            return backends.Result(text="k", model="gemini:x")
        with mock.patch.object(backends, "_gemini", spy):
            r = backends.call("gemini:x", "s", "key AIzaSyA1234567890abcdefghij", self.eng.cfg)
        self.assertNotIn("AIzaSy", seen[0])
        self.assertEqual(r.extra["redacted"], 1)

    def test_local_backend_keeps_text(self):
        seen = []

        def spy(model, system, prompt, cfg, max_tokens=None):
            seen.append(prompt)
            return backends.Result(text="<think>x</think>hi", model="ollama:x")
        with mock.patch.object(backends, "_ollama", spy):
            r = backends.call("ollama:x", "s", "key AIzaSyA1234567890abcdefghij", self.eng.cfg)
        self.assertIn("AIzaSy", seen[0])
        self.assertEqual(r.text, "hi")


class TestChoice(Base):
    def test_global_ledger_folder_is_not_a_project(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            (home / ".ctxflow").mkdir()
            (home / ".ctxflow" / "usage.jsonl").write_text("")
            work = home / "work"
            work.mkdir()
            found = Project.find(work)
            self.assertEqual(found.root.resolve(), work.resolve())
            self.assertFalse(found.initialized)

    def test_fast_mode_keeps_the_advisor_silent(self):
        self.run_cli("mode", "fast")
        with mock.patch.object(backends, "call", side_effect=AssertionError):
            out, err = self.run_cli("advise", "plan", "next?")
        self.assertEqual(out, "")
        self.assertIn("Fast mode", err)

    def test_profile_changes_routing_and_can_be_cleared(self):
        self.run_cli("profile", "set", "local")
        cfg = config.load(self.root)
        self.assertEqual(cfg["routing"]["light"], ["ollama:auto"])
        self.assertEqual(config.council_seats(cfg)[1], "ollama:auto")
        self.run_cli("profile", "clear")
        self.assertEqual(config.load(self.root)["routing"]["light"], ["echo:light"])

    def test_unknown_profile_rejected(self):
        with self.assertRaises(SystemExit):
            self.run_cli("--profile", "nope", "status")

    def test_status_line_makes_no_calls(self):
        with mock.patch.object(backends, "call", side_effect=AssertionError):
            out, _ = self.run_cli("line")
        self.assertIn("advisor 0/3", out)


if __name__ == "__main__":
    unittest.main()
