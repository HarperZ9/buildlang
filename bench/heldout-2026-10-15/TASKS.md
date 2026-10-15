Held-out BuildLang tasks (bench4)

Write one complete, standalone BuildLang program per task, in a file named
`<id>.bld` (for example `collatz.bld`). Each program prints exactly the
expected output to standard output and exits with status 0 unless the task
says otherwise. Input files named in a task sit in the current working
directory when the program runs. Trailing spaces at line ends are ignored;
leading spaces are not.

1. collatz: For each starting value n from 1 to 30 inclusive, count the steps
   the Collatz rule takes to reach 1 (n even: n/2; n odd: 3n+1; n=1 takes 0
   steps). Print the start with the most steps and its step count, separated
   by one space. On a tie, pick the smallest start.

2. caesar: Shift every lowercase letter of "attack at dawn zebra" forward by 3,
   wrapping z to c; leave spaces unchanged. Print the encoded text on one line,
   then decode it back and print the result on the next line.

3. ledger: Accounts 1, 2 and 3 start with balances 100, 50 and 0. Model an
   account as a struct and a transaction as a sum type with variants Deposit
   (account, amount), Withdraw (account, amount) and Transfer (from, to,
   amount). Apply in order: Deposit 3 25; Withdraw 2 80; Transfer 1 to 3 60;
   Withdraw 1 50; Transfer 2 to 1 50; Deposit 2 5. A withdraw or transfer
   larger than the source balance is rejected and changes nothing. Print
   "<id> <balance>" for each account in id order, then "rejected <count>".

4. matmul: Multiply A = [[1,2,3],[4,5,6]] by B = [[7,8],[9,10],[11,12]]. Print
   each row of the product on its own line, values separated by one space.

5. roman: Convert 1994, 2024, 3999, 4, 9 and 40 to Roman numerals, one per
   line, in that order.

6. bsearch: Implement binary search over the sorted list
   [2, 5, 8, 12, 16, 23, 38, 56, 72, 91]. For the targets 23, 2, 91, 7, 100 in
   that order print the index of the target, or -1 if absent, one per line.

7. palindromes: For each word of racecar, level, buildlang, noon, ab, a print
   "<word> yes" if it reads the same backwards, else "<word> no", one per line.

8. rle: Run-length encode "aaabccddddde" as letter followed by count
   (expected form: a3b1...). Print it, then on the next line print the
   encoding of "z".

9. rpn: Evaluate the reverse Polish expressions "3 4 + 2 * 7 -" and
   "5 1 2 + 4 * + 3 -". Tokens are separated by single spaces; numbers are
   non-negative integers; operators are + - *. Print each result on its own
   line.

10. config: Read the file app.conf. Lines starting with # are comments and
    blank lines are skipped; every other line is key=value with optional spaces
    around the key and the value. Print "<key>: <value>" for each entry in file
    order with spaces trimmed, then "total <n>" where n is port plus timeout as
    integers.

11. csv: Read sales.csv. The first line is the header item,qty,price; each
    other line is one item. Revenue per item is qty times price. Print
    "total <sum of revenue>" then "top <item> <revenue>" for the item with the
    highest revenue.

12. squares_file: Write the squares of 1 to 6 to the file squares.txt, one per
    line. Then read squares.txt back and print "lines <count>" and
    "last <text of the last line>" on two lines.

13. ring: Implement a ring buffer of integers with capacity 3. Pushing onto a
    full buffer overwrites the oldest value; pop removes and returns the
    oldest. Push 1, 2, 3, 4, 5; pop once and print "popped <value>"; push 6;
    then print the remaining values oldest first, separated by one space.

14. bst: Insert 50, 30, 70, 20, 40, 60, 80, 35 in that order into a binary
    search tree built from a recursive type. Print the in-order traversal on
    one line separated by spaces, then "height <h>" where h counts the nodes
    on the longest root-to-leaf path.

15. traffic: Model a traffic light as a sum type Red, Green, Yellow with the
    cycle Red to Green to Yellow to Red, and durations Red 30, Green 25,
    Yellow 5 seconds. Starting at Red, print the first 5 states as
    "<State> <seconds>", one per line, then "total <seconds>" for those 5.

16. bits: Print 2026 in binary (no prefix, no leading zeros), then on the next
    line the number of 1 bits in it.

17. leap: Count the Gregorian leap years from 1900 to 2100 inclusive and print
    the count.

18. digits: Compute 2 to the power 20 with a loop and print it. On the next
    line print the sum of its decimal digits, and on the line after that its
    digital root (repeat the digit sum until one digit remains).

19. anagram: For the pairs (listen, silent), (hello, world), (evil, vile),
    (aab, abb) print "<a> <b> yes" if the two words are anagrams, else
    "<a> <b> no", one per line.

20. pascal: Print the first 6 rows of Pascal's triangle, one row per line,
    values separated by one space, with no leading spaces. The first row is 1.

21. inventory: Define an item struct with name, price and quantity. The stock
    is apple 120 qty 3, pear 95 qty 0, plum 40 qty 12. Write a lookup function
    that returns an optional item by name. For the queries plum, kiwi, pear in
    that order print "<name> <price> x<qty>" when the item exists with
    quantity above 0, "<name> out of stock" when it exists with quantity 0,
    and "<name> missing" when it does not exist.

22. parse_fields: Split "12,x,7,30,-4,9z" on commas. Write a function that
    parses one field as a signed decimal integer and returns a result type
    that is an error for anything that is not a valid integer. Print
    "sum <sum of valid fields>" then "invalid <count of invalid fields>".

23. higher_order: Write map, filter and fold functions that each take a
    function as a parameter. Using them on the integers 1 to 10, print the sum
    of the squares of the even numbers on one line, and the product of the odd
    numbers on the next.

24. exit_status: Check whether the list [3, 7, 7, 12, 5] is in non-decreasing
    order. If it is, print "sorted" and exit with status 0. If it is not,
    print "unsorted at index <i>", where i is the first zero-based index whose
    value is smaller than the value before it, and exit with status 3.

25. stats: For the values 2, 3, 5, 7, 11, 13 print "mean <m>" with the mean
    rounded to exactly two decimal places, then "min <min> max <max>".

26. stairs: Print a right-aligned staircase of # characters with 4 rows: row k
    (1 to 4) has 4-k spaces followed by k # characters.

27. largest: Write one generic function that returns the largest element of a
    non-empty list, and use it on the integers [3, 9, 4] and on the strings
    ["pear", "apple", "plum"] (alphabetical order). Print the two results on
    two lines.
