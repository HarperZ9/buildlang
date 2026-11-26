# Tier 1: Boot-time partial evaluation (Futamura specialization)
define_method(:double) { |x| x * 2 }
define_method(:bump) { |x| x + 7 }

def main
    # Tier 1 specialized static call
    puts double(11)
    # Tier 2 closed-world dynamic dispatch table
    puts send("double", 21)
    puts send("bump", 35)
    # Tier 3 fuel-metered capability-bounded pure arithmetic sandbox
    puts eval("10 + 25 + 7")
end
main
