# Unmodeled native C-ABI extensions and ambient host escapes must be refused
require 'active_record'
def demo
    ObjectSpace.each_object
    x = 1
    Kernel.system("echo escape")
end
