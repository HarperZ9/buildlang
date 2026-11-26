function collatz(n) {
    let x = n;
    let steps = 0;
    while (x != 1) {
        if (x % 2 == 0) {
            x = x / 2;
        } else {
            x = 3 * x + 1;
        }
        steps = steps + 1;
    }
    return steps;
}
function main() {
    console.log(collatz(27));
    console.log(collatz(9));
}
main();
