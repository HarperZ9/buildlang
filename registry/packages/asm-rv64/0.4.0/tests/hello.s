# A program for the serial run of emu-rv64 0.7.0. It sends the 14 bytes of
# "hello, serial" and a line feed to the serial device and halts with
# exit code 0.
#
# The halt word is `jal x0, 0`, a jump to itself. The serial run stops
# when pc reaches it and reports x10 as the exit code.
#
# Steps, worked out by hand: 3 before the loop, 5 for each of the 14
# bytes, then lbu and the taken beq for the zero byte, then addi:
# 3 + 14 * 5 + 2 + 1 = 76 steps.

        lui     x5, 0x10000         # x5 = 0x10000000, the serial device
        auipc   x6, 0               # x6 = the address of this instruction, offset 4
        addi    x6, x6, 36          # x6 = offset 40, the text
next:   lbu     x7, 0(x6)
        beq     x7, x0, done        # a zero byte ends the text
        sb      x7, 0(x5)
        addi    x6, x6, 1
        jal     x0, next
done:   addi    x10, x0, 0          # exit code 0
        jal     x0, 0               # the halt word, 0x0000006F
text:   .word   0x6C6C6568          # "hell", least significant byte first
        .word   0x73202C6F          # "o, s"
        .word   0x61697265          # "eria"
        .word   0x00000A6C          # "l", line feed, two zero bytes
