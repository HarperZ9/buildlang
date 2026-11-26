# Prime counting and summation using while, for range, if, break, continue.
def is_prime(n: int) -> bool:
    if n < 2:
        return False
    d: int = 2
    while d * d <= n:
        if n % d == 0:
            return False
        d += 1
    return True

def main():
    count: int = 0
    total: int = 0
    for k in range(2, 200):
        if k % 2 == 0 and k != 2:
            continue
        if is_prime(k):
            count += 1
            total += k
    print(count, total)

if __name__ == "__main__":
    main()
