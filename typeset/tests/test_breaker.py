"""Known-answer tests for the Knuth-Plass breaker."""
import itertools
import os
import random
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from engine.breaker import (Box, Glue, Penalty, Params, badness, break_lines, INF_PENALTY, EJECT,  # noqa: E402
                            INF_BAD, SP)


def para(words, space=(6, 3, 2), hyphen_at=()):
    items = []
    for i, w in enumerate(words):
        if i:
            items.append(Glue(space[0] * SP, space[1] * SP, space[2] * SP))
        items.append(Box(w * SP))
        if i in hyphen_at:
            items.append(Penalty(2 * SP, 50, True))
            items.append(Box(w * SP // 2))
    items += [Penalty(0, INF_PENALTY), Glue(0, 1, 0, stretch_order=1), Penalty(0, EJECT)]
    return items


def brute_force(items, width, params):
    """Independent oracle: try every subset of legal breakpoints, score with TeX's rules."""
    legal = [i for i, it in enumerate(items)
             if (isinstance(it, Penalty) and it.penalty < INF_PENALTY and i != len(items) - 1)
             or (isinstance(it, Glue) and i > 0 and isinstance(items[i - 1], Box))]
    best = None
    for r in range(len(legal) + 1):
        for combo in itertools.combinations(legal, r):
            seq, total, ok = list(combo) + [len(items) - 1], 0, True
            prev, prev_fit, prev_flag = -1, 2, False
            for b in seq:
                s = prev + 1
                while s < len(items) - 1 and not isinstance(items[s], Box):
                    s += 1
                nat = sum(it.width for it in items[s:b] if not isinstance(it, Penalty))
                it = items[b]
                pen, flag = (it.penalty, it.flagged) if isinstance(it, Penalty) else (0, False)
                nat += it.width if isinstance(it, Penalty) else 0
                st = sum(it.stretch for it in items[s:b] if isinstance(it, Glue) and it.stretch_order == 0)
                fil = any(isinstance(it, Glue) and it.stretch_order for it in items[s:b])
                sh = sum(it.shrink for it in items[s:b] if isinstance(it, Glue))
                if nat < width:
                    bad = 0 if fil else badness(width - nat, st)
                    fit = 2 if fil else (0 if bad > 99 else 1 if bad > 12 else 2)
                elif nat - width > sh:
                    ok = False
                    break
                else:
                    bad = badness(nat - width, sh)
                    fit = 3 if bad > 12 else 2
                if bad > params.tolerance:
                    ok = False
                    break
                d = (params.line_penalty + bad) ** 2
                d += pen * pen if pen > 0 else (-pen * pen if pen > EJECT else 0)
                if flag and prev_flag:
                    d += params.double_hyphen_demerits
                if abs(fit - prev_fit) > 1:
                    d += params.adj_demerits
                if pen == EJECT and prev_flag:
                    d += params.final_hyphen_demerits
                total += d
                prev, prev_fit, prev_flag = b, fit, flag
            if ok and (best is None or total < best[0]):
                best = (total, seq)
    return best


class Badness(unittest.TestCase):
    def test_tex_values(self):
        # tex.web section 108: badness(t, s) ~ 100 (t/s)^3, integer arithmetic
        self.assertEqual(badness(0, 0), 0)
        self.assertEqual(badness(5, 0), INF_BAD)
        self.assertEqual(badness(10, 10), 100)
        self.assertEqual(badness(5, 10), 12)
        self.assertEqual(badness(13, 10), 219)
        self.assertEqual(badness(100, 10), INF_BAD)


class KnuthPlass(unittest.TestCase):
    def test_matches_brute_force(self):
        rng = random.Random(1981)
        params = Params(tolerance=10000, pretolerance=-1)
        for trial in range(60):
            words = [rng.randint(8, 40) for _ in range(rng.randint(4, 9))]
            hy = {i for i in range(len(words)) if rng.random() < 0.2}
            items = para(words, hyphen_at=hy)
            width = rng.randint(60, 120) * SP
            oracle = brute_force(items, width, params)
            try:
                res = break_lines(items, width, params, has_hyphens=False)
            except RuntimeError:
                self.assertIsNone(oracle, f"trial {trial}: breaker found nothing, oracle did")
                continue
            if oracle is None:
                self.assertTrue(res.overfull, f"trial {trial}: oracle infeasible, breaker must report overfull")
                continue
            self.assertEqual(res.demerits, oracle[0], f"trial {trial}")

    def test_optimal_beats_greedy(self):
        # first-fit would leave a very loose second line; the optimum balances the two
        items = para([30, 30, 30, 30, 50])
        res = break_lines(items, 140 * SP, Params(pretolerance=-1, tolerance=10000))
        self.assertEqual(len(res.breaks), 2)
        self.assertLess(max(res.badness), 100)

    def test_overfull_reported(self):
        items = para([20, 300, 20])
        res = break_lines(items, 100 * SP, Params(pretolerance=-1, tolerance=200))
        self.assertTrue(res.overfull)
        line, excess = res.overfull[0]
        self.assertGreater(excess, 150)


if __name__ == "__main__":
    unittest.main()
