import io

N = "\r\n"


def crlf(t):
    return t.replace("\r\n", "\n").replace("\n", N)


names = """print println eprint eprintln assert
printf fprintf sprintf snprintf vprintf vfprintf vsprintf vsnprintf scanf fscanf sscanf
puts fputs gets fgets putc fputc putchar getc fgetc getchar ungetc perror
fopen freopen fclose fflush fread fwrite fseek ftell rewind fgetpos fsetpos feof ferror clearerr
tmpfile tmpnam setbuf setvbuf
malloc calloc realloc free _Exit quick_exit at_quick_exit
strtoul strtoll strtoull strtof strtold atoll mblen mbtowc wctomb mbstowcs wcstombs
memcpy memmove memset memcmp memchr strlen strcpy strncpy strcat strncat strcmp strncmp strcoll
strxfrm strchr strrchr strstr strspn strcspn strpbrk strtok strerror strdup strndup
sqrt cbrt sin cos tan asin acos atan atan2 sinh cosh tanh asinh acosh atanh exp exp2 expm1
log log2 log10 log1p pow fabs floor ceil round trunc rint nearbyint lround llround lrint llrint
fmod remainder remquo fmax fmin fdim fma hypot copysign nan nextafter nexttoward frexp ldexp
modf scalbn scalbln ilogb logb erf erfc tgamma lgamma j0 j1 jn y0 y1 yn gamma signgam
sqrtf sinf cosf tanf powf expf logf fabsf floorf ceilf roundf fmodf
time clock difftime mktime strftime gmtime localtime asctime ctime timespec_get
isalnum isalpha isblank iscntrl isdigit isgraph islower isprint ispunct isspace isupper isxdigit
signal raise setjmp longjmp errno
read write open close creat lseek pipe dup dup2 fork execv execve execvp wait waitpid kill
sleep usleep alarm pause access chdir getcwd rmdir mkdir unlink link symlink readlink stat fstat
lstat chmod chown truncate ftruncate isatty getpid getppid getuid geteuid getgid
socket bind listen accept connect send recv sendto recvfrom shutdown select poll ioctl fcntl
index rindex bcopy bzero bcmp ffs random srandom drand48 lrand48 srand48 basename dirname
mmap munmap mprotect dlopen dlsym dlclose environ optarg optind getopt
stdin stdout stderr""".split()

arms = ""
line = "            "
first = True
for n in names:
    piece = ("" if first else "| ") + '"' + n + '" '
    first = False
    if len(line) + len(piece) > 96:
        arms += line.rstrip() + "\n"
        line = "                "
    line += piece
arms += line.rstrip() + "\n"

helper = crlf(
    """    /// True when a top-level user function of this name would collide with a
    /// name the lowerer or the C output already uses: a C library function, a
    /// runtime helper or a print builtin. Such a function is lowered under
    /// `user_fn_c_name`, so `fn log(...)`, `fn read(...)` or `fn printf(...)`
    /// in a program is the program's own function at every call.
    /// `min`, `max`, `abs` and the names in the C backend's stdlib-collision
    /// list are escaped there and are not repeated here.
    pub(crate) fn shadows_runtime_name(name: &str) -> bool {
        if name.starts_with("build_") || name.starts_with("__") {
            return true;
        }
        matches!(
            name,
"""
    + arms
    + """        )
    }

    /// The name a shadowing user function is lowered under.
    pub(crate) fn user_fn_c_name(name: &str) -> Arc<str> {
        Arc::from(format!("bld_u_{name}"))
    }

    /// Top-level functions with a body whose names collide (see
    /// `shadows_runtime_name`), renamed for lowering. Declarations without a
    /// body and `extern` functions keep their names: those name C symbols.
    fn rename_shadowing_fns(&mut self, items: &[ast::Item]) -> Vec<ast::Item> {
        items
            .iter()
            .map(|item| {
                let ItemKind::Function(f) = &item.kind else {
                    return item.clone();
                };
                let name = f.name.name.as_ref();
                if f.body.is_none() || f.sig.abi.is_some() || !Self::shadows_runtime_name(name) {
                    return item.clone();
                }
                self.shadow_fns.insert(f.name.name.clone());
                let mut f2 = f.as_ref().clone();
                f2.name = ast::Ident {
                    name: Self::user_fn_c_name(name),
                    span: f.name.span,
                };
                let mut item2 = item.clone();
                item2.kind = ItemKind::Function(Box::new(f2));
                item2
            })
            .collect()
    }

"""
)

p = "compiler/src/codegen/lower/mod.rs"
s = io.open(p, newline="").read()
anchor = crlf(
    """    pub fn lower_module(mut self, module: &ast::Module) -> CodegenResult<MirModule> {
        // Register built-in vector math types before user code
        self.register_vector_types();
"""
)
assert s.count(anchor) == 1
k = s.index(anchor)
end = s.index(crlf("        Ok(self.module.build())\n    }\n"), k)
body = s[k:end]
assert body.count("&module.items") == 5
body2 = body.replace("&module.items", "&items")
body2 = body2.replace(
    crlf("        self.register_vector_types();\n"),
    crlf(
        """        self.register_vector_types();

        // A user function named like a C library or runtime function is
        // lowered under its own name space.
        let items = self.rename_shadowing_fns(&module.items);
"""
    ),
    1,
)
s = s[:k] + helper + body2 + s[end:]

old = crlf(
    """                return prefixed;
            }
        }
        Arc::from(bare_name)
    }
"""
)
assert s.count(old) == 1
s = s.replace(
    old,
    crlf(
        """                return prefixed;
            }
        }
        if self.shadow_fns.contains(bare_name) {
            return Self::user_fn_c_name(bare_name);
        }
        Arc::from(bare_name)
    }
"""
    ),
)
old = crlf("    pub(crate) char_vec_locals: std::collections::HashSet<LocalId>,\n")
assert s.count(old) == 1
s = s.replace(
    old,
    old
    + crlf(
        """    /// Top-level user functions lowered under `user_fn_c_name` because their
    /// names collide with a C library or runtime name.
    pub(crate) shadow_fns: std::collections::HashSet<Arc<str>>,
"""
    ),
)
old = crlf("            char_vec_locals: std::collections::HashSet::new(),\n")
assert s.count(old) == 2
s = s.replace(old, old + crlf("            shadow_fns: std::collections::HashSet::new(),\n"))
io.open(p, "w", newline="").write(s)

p = "compiler/src/codegen/lower/expr.rs"
s = io.open(p, newline="").read()
old = crlf(
    "    fn lower_call(&mut self, func: &ast::Expr, args: &[ast::Expr]) -> CodegenResult<MirValue> {\n"
)
assert s.count(old) == 1
s = s.replace(
    old,
    old
    + crlf(
        """        // A call to a user function that shares a name with a C library or
        // runtime function goes to the user's function (see `shadow_fns`).
        if let Some(renamed) = self.shadowed_callee(func) {
            return self.lower_call(&renamed, args);
        }
"""
    ),
)
old = crlf(
    """    pub(crate) fn lower_ident(&mut self, ident: &ast::Ident) -> CodegenResult<MirValue> {
        if let Some(&local) = self.var_map.get(&ident.name) {
            self.note_pending_vec_use(local);
            Ok(values::local(local))
        } else {
"""
)
assert s.count(old) == 1
s = s.replace(
    old,
    crlf(
        """    /// `func` with its name replaced, when it names a shadowing user function
    /// (see `shadow_fns`) and no local variable of that name is in scope.
    fn shadowed_callee(&self, func: &ast::Expr) -> Option<ast::Expr> {
        let name = match &func.kind {
            ExprKind::Ident(ident) => ident.name.clone(),
            ExprKind::Path(path) if path.segments.len() == 1 => path.last_ident()?.name.clone(),
            _ => return None,
        };
        if !self.shadow_fns.contains(&name) || self.var_map.contains_key(&name) {
            return None;
        }
        let mut renamed = func.clone();
        renamed.kind = ExprKind::Ident(ast::Ident {
            name: MirLowerer::user_fn_c_name(&name),
            span: func.span,
        });
        Some(renamed)
    }

"""
    )
    + old
    + crlf(
        """            if self.shadow_fns.contains(&ident.name) {
                let renamed = ast::Ident {
                    name: MirLowerer::user_fn_c_name(&ident.name),
                    span: ident.span,
                };
                return self.lower_ident(&renamed);
            }
"""
    ),
)
io.open(p, "w", newline="").write(s)
print("applied")
