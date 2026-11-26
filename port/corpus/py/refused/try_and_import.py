# Refused Python constructs: non-math import and try/except block.
import json

def main():
    try:
        x: int = 1
    except:
        x: int = 2
    print(x)
