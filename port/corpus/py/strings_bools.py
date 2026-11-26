# Strings, len(), boolean logic, and transitive Console effect inference.
def report_flag(label: str, flag: bool, n: int):
    print(label, flag, n)

def main():
    a: str = "build"
    b: str = "lang"
    full: str = a + "-" + b
    ok: bool = (len(full) == 10) and not (len(a) < 3)
    report_flag(full, ok, len(full))
    report_flag("neg", not ok, len(b))

if __name__ == "__main__":
    main()
