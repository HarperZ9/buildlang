# Dynamic Rails and Ruby metaprogramming must be refused
require 'active_record'
def demo
    method_missing(:foo)
    x = 1
    eval("x + 2")
end
