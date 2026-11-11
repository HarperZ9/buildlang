# The serial program of `emu-rv64 cases` (program_serial in src/main.bld of
# 0.4.0), as text. It sends the bytes 4F 4B 0A, "OK" and a line feed, to
# the serial device and stops on EBREAK.

        lui     x5, 0x10000         # x5 = 0x10000000, the serial device
        addi    x6, x0, 0x4F        # "O"
        sb      x6, 0(x5)
        addi    x6, x0, 0x4B        # "K"
        sb      x6, 0(x5)
        addi    x6, x0, 10          # line feed
        sb      x6, 0(x5)
        ebreak
