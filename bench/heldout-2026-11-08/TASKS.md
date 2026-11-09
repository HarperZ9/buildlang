Held-out BuildLang tasks (bench5, M3 probe of 2026-11-08)

Write one complete, standalone BuildLang program per task, in a file named
`<id>.bld` (for example `fizzsum.bld`). Each program prints exactly the
expected output to standard output and exits with status 0 unless the task
says otherwise. Input files named in a task sit in the current working
directory when the program runs. Trailing spaces at line ends are ignored;
leading spaces are not. Numbers are printed in plain decimal with no
thousands separators.

1. fizzsum: Sum the integers from 1 to 1000 inclusive that are divisible by 3
   or by 5 but not by both. Print the sum.

2. primes: Count the primes below 5000 and find the largest prime below 5000.
   Print "<count> <largest>".

3. gcd_lcm: For the pairs (12, 18), (17, 5), (100, 75) and (270, 192), in
   that order, print "<a> <b> <gcd> <lcm>", one pair per line.

4. wordfreq: Count the words of "the cat and the hat and the bat sat on the
   mat". Print "<word> <count>" for each distinct word, sorted by count from
   high to low, and by word in alphabetical order when counts are equal.

5. reverse_words: Print the words of "one two three four five" in reverse
   order on one line, separated by single spaces. On the next line print the
   same sentence with every word spelled backwards but the word order kept.

6. transpose: Transpose the 3 by 4 matrix with rows [1, 2, 3, 4],
   [5, 6, 7, 8] and [9, 10, 11, 12]. Print each row of the result on its own
   line, values separated by one space.

7. bubble: Sort [29, 3, 17, 8, 42, 1, 15] in ascending order with bubble sort,
   counting every swap of two adjacent elements. Print the sorted values on
   one line separated by spaces, then "swaps <n>" on the next line.

8. fib: Print the 90th Fibonacci number, where the 1st and 2nd are both 1.
   On the next line print the sum of the 1st to the 20th Fibonacci numbers.

9. brackets: For each of the strings "([]{})", "([)]", "((", "{[()()]}" and
   "" (the empty string), in that order, print "yes" if every bracket is
   closed by the matching kind in the right order, else "no". One answer per
   line.

10. binary: Print the binary form of 37, 255, 1024 and 0, one per line, with
    no leading zeros (0 prints as "0").

11. hex_parse: Convert the hexadecimal strings "ff", "1A3", "0" and "7fff"
    (letters may be upper or lower case) to decimal by hand, without a
    library parse call. Print one value per line.

12. josephus: Ten people stand in a circle, numbered 1 to 10. Counting starts
    at person 1, and every third person counted leaves the circle; counting
    continues with the next person still in the circle. Print the numbers in
    the order people leave, separated by spaces, on one line.

13. shapes: Model a shape as a sum type with variants Rect (width, height) and
    Square (side), with integer sizes. For the list Rect 3 4, Square 5,
    Rect 2 10, Square 1, print "<area> <perimeter>" for each shape on its own
    line, then "total area <sum of areas>".

14. stock: Model an item as a struct with a name, a quantity and a unit price
    in cents. Start with apple (10, 50), pear (4, 75) and plum (0, 120).
    Apply in order: sell apple 3; restock plum 6; sell pear 5; sell plum 2;
    restock apple 1. A sale larger than the quantity in stock is refused and
    changes nothing. Then print "<name> <quantity> <stock value in cents>" for
    each item in the original order, and "refused <count>".

15. compose: Write three functions, double (times 2), add3 (plus 3) and square
    (times itself). Put them in an ordered collection or pass them as values,
    and apply them in that order (double, then add3, then square) to each of
    1, 2, 3, 4 and 5. Print the five results on one line separated by spaces.

16. first_over: In the list [12, 7, 33, 51, 8, 77, 64] find the first value
    greater than 50 and the first value greater than 100, each with a function
    that returns an optional value. Print each result on its own line,
    printing "none" when there is no such value.

17. parse_sum: Split "12,x,7,,40,-3,5a" on commas. Each piece that is a
    decimal integer (an optional leading minus sign followed by one or more
    digits) counts as valid; every other piece, the empty one included, is an
    error. Use a function that returns a result type (success or error) for
    each piece. Print "sum <sum of valid values>" and then "errors <count>".

18. score_check: Read the file scores.txt, which holds one integer per line.
    If any score is above 100, print "invalid <first such score>" and exit
    with status 3. Otherwise print "average <integer average, rounded down>"
    and exit with status 0.

19. upper_file: Read the file words.txt (one word per line, lowercase ASCII)
    and write the file upper.txt with the same words in uppercase, one per
    line, in the same order. Print "lines <n>" where n is the number of words.

20. histogram: For the labels a, b, c and d with the values 3, 7, 2 and 5,
    print one line per label: the label, a space, then as many "#" characters
    as the value.

21. prefix: Print the longest common prefix of the words "interstellar",
    "internet", "interval" and "internal". On the next line print the longest
    common prefix of "dog" and "cat", or "(none)" if it is empty.

22. isbn: An ISBN-10 is valid when its ten characters are digits (the last may
    also be "X", worth 10) and the sum of each digit times its weight (10 for
    the first character down to 1 for the last) is divisible by 11. For
    "0306406152", "0306406153", "080442957X" and "12345" print
    "<isbn> valid" or "<isbn> invalid", one per line.

23. wrap: Wrap the text "the quick brown fox jumps over the lazy dog and
    keeps running far away" into lines of at most 16 characters, breaking only
    at spaces and putting as many words on each line as fit. Print each line.

24. hanoi: Print the number of moves needed to solve the Towers of Hanoi with
    10 disks. Then, solving with 3 disks recursively from peg A to peg C using
    peg B, print each move as "<from> <to>" on its own line.

25. expr_tree: Build the expression (2 + 3) * (4 - 1) + 10 as a recursive tree
    (a sum type whose operator variants hold their two operands) and evaluate
    it recursively. Print the value, then print the expression in fully
    parenthesised form: "(((2 + 3) * (4 - 1)) + 10)".

26. colors: Count the colours in "red blue red green blue red yellow" with a
    map from string to count. Print "<colour> <count>" for each colour in the
    order it first appears.

Game-shaped tasks

27. projectile: A fixed-step simulation in integer units. A ball starts at
    height 0 with upward speed 1000. Each step first lowers the speed by 98,
    then adds the speed to the height. The step counter starts at 0 and
    increases by one each step. Stop after the first step that leaves the
    height below 0. Print "landed <steps>" and then "peak <the greatest height
    reached after any step>".

28. particles: Five particles move in a 1D box from 0 to 100 inclusive. They
    start at positions 5, 20, 50, 77, 95 with velocities 7, -9, 13, 4, -6.
    Each step, every particle moves by its velocity; a particle that ends a
    step below 0 is placed at minus its position (reflected) and its velocity
    is negated, and one that ends above 100 is placed at 200 minus its
    position and its velocity is negated; each such reflection counts as one
    bounce. Run 50 steps. Print the final positions on one line separated by
    spaces, then "bounces <total>".

29. units_table: Read units.csv. The first line is the header
    name,hp,atk,def; each other line is one unit. The damage of an attacker on
    a defender is the attacker's atk minus the defender's def, but at least 1.
    The hits an attacker needs against a defender is the defender's hp divided
    by that damage, rounded up. For each unit in file order, consider every
    other unit as a target and print "<name> <target> <hits>" for the target
    that takes the fewest hits; on a tie pick the target whose name comes
    first alphabetically.

30. duel_trace: Two fighters fight in fixed steps. A has hp 30, attack 7 and
    cooldown 3; B has hp 40, attack 5 and cooldown 2. Each fighter's timer
    starts at its cooldown. At each tick (the first tick is 1) both timers
    drop by 1; every fighter whose timer reaches 0 attacks the other and its
    timer is reset to its cooldown. Both attacks in one tick happen at the
    same time, so a fighter that drops to 0 hp or below in a tick still lands
    its attack in that tick. After the attacks, if either fighter has hp of 0
    or less the fight ends. For each attack, in the order A then B within a
    tick, write one line to the file duel.trace as a JSON object with exactly
    these keys in this order and no spaces: {"t":<tick>,"by":"<A or B>",
    "hp":<the target's hp after the hit>}. Then print "winner <A, B or draw>
    tick <t>", where draw means both fighters ended at 0 hp or below.

31. dice: Use the generator x = (1103515245 * x + 12345) mod 2147483648 with
    seed x = 42. For each of 1000 rolls, advance x first and then roll
    1 + (x / 65536) mod 6, using integer division. Print the count of each
    face from 1 to 6 as "<face> <count>", one per line.

32. grid_path: Read map.txt, a grid of characters where "#" is a wall, "." is
    floor, "S" the start and "E" the exit. Moves go one cell up, down, left or
    right onto any cell that is not a wall. Print the fewest moves from S to E,
    or "unreachable" if there is no path.

33. life: Run Conway's Game of Life on an 8 by 8 grid whose cells outside the
    grid are always dead. The starting live cells (row, column, counting from
    0 at the top left) are (0,1), (1,2), (2,0), (2,1), (2,2). Run 4
    generations, then print the grid as 8 lines of 8 characters, "#" for a
    live cell and "." for a dead one, and then "alive <count>".

34. inventory_log: Read events.txt, one event a line in the form
    "<action> <item> <count>" with action pickup, use or drop. Pickup adds the
    count to the item; use and drop subtract it, but an event that would take
    an item below 0 is ignored and counted as refused. Print "<item> <count>"
    for every item with a count above 0, sorted by item name, then
    "refused <n>".
