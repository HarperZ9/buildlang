fn bad(x: i64) i64 {
    comptime var k: i64 = 3;
    var y: i64 = x;
    var p = @ptrCast(*i64, y);
    return p;
}
