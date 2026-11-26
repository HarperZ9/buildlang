def tri(n)
    s = 0
    i = 1
    while i <= n
        s = s + i
        i = i + 1
    end
    return s
end
def main
    puts tri(20)
end
main
