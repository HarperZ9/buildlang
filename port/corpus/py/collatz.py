# Collatz stopping times with elif, integer division //, and stepped range.
def collatz_steps(n: int) -> int:
    steps: int = 0
    while n > 1:
        if n % 2 == 0:
            n = n // 2
        elif n % 4 == 1:
            n = 3 * n + 1
        else:
            n = 3 * n + 1
        steps += 1
    return steps

def main():
    best_n: int = 1
    best_s: int = 0
    for x in range(3, 50, 2):
        s: int = collatz_steps(x)
        if s > best_s:
            best_s = s
            best_n = x
    print(best_n, best_s)
    for down in range(10, 0, -3):
        print(down)

if __name__ == "__main__":
    main()
