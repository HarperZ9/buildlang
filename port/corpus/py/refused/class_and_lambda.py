# Refused Python constructs: class and lambda must be rejected with exact line numbers.
class Box:
    pass

def main():
    f = lambda x: x + 1
    print(1)
