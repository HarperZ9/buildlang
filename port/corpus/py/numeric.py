# Mixed int/float arithmetic, math functions, and formatted f-string output.
import math

def hyp(a: float, b: float) -> float:
    return math.sqrt(a * a + b * b)

def damped(t: float, rate: float) -> float:
    return math.exp(-rate * t) * math.cos(t * 1.5)

def main():
    h: float = hyp(3.0, 4.0)
    print(f"hyp={h:.6f}")
    acc: float = 0.0
    for i in range(1, 6):
        v: float = damped(float(i) * 0.5, 0.25)
        acc += math.fabs(v)
        print(f"i={i} v={v:.6f}")
    print(f"acc={acc:.6f}")

if __name__ == "__main__":
    main()
