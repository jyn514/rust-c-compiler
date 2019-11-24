mod utils;

#[test]
fn test_decl_and_func_definition() {
    utils::assert_compile_error("int i, main(void) { return 0; }");
    utils::assert_compile_error("int main(void) { return 0; }, i;");
    utils::assert_succeeds("int f(), i; int main(void) { return 0; }");
}

#[test]
fn test_decl_after_init() {
    utils::assert_succeeds("int main(); int main() {} int main();");
    utils::assert_succeeds("int g(int); int g(int i) { return i; } int main() { return g(0); }")
}

#[test]
fn test_different_types() {
    utils::assert_compile_error("int i; long i;");
}

#[test]
fn test_initializers() {
    utils::assert_compile_error("char c = '🙃';");
    utils::assert_compile_error("int a[-1];");
    utils::assert_compile_error("int i = {};");
    utils::assert_succeeds(
        "
    int i = 1;
    int a[3] = {1, 2, 3};
    int b[2][2] = {{1, 2}, {3, 4}};
    double d = 1.2;
    float f = 1.2;
    short s = 31;
    unsigned u = -1;
    char c = 'h';
    char h[] = \"hi there\";
    int main(void) { return 0; }
    ",
    );
}

#[test]
fn static_initializers() {
    utils::assert_code(
        "int i = 0,
        *p = &i,
        *q = 0,
        *r = (void*)0,
        f(void),
        (*fp)() = f,
        a[] = {1, 2, 3};
        struct s { int i; } my_s;
        int *s = &my_s.i;
    int f() { return 1; }
    int main() { return (*fp)(); }",
        1,
    );
}

#[test]
#[ignore]
fn struct_initializers() {
    utils::assert_code(
        "struct s {
        int i;
        float f;
        union { int a; float b; } u;
    } m = { 1, 2.4, 3 };
    int main() { return m.i; }",
        1,
    );
}

#[test]
fn union_initializers() {
    utils::assert_code(
        "
    union { int a; float b; } u = 3;
    int main() { return u.a; }",
        3,
    );
}

#[test]
fn multiple_initializers() {
    utils::assert_code(
        "int main() {
        int i = 1, *p = &i;
        return *p;
}",
        1,
    );
}

#[test]
fn abstract_param_in_definition() {
    utils::assert_compile_error("int f(int) { return 1; }");
}

#[test]
fn bad_signature_for_main() {
    utils::assert_compile_error("int main(int);");
    utils::assert_compile_error("int main(char **);");
    utils::assert_compile_error("int main(int, char **, ...);");
    utils::assert_succeeds("int main(int argc, char ** argv) {}");
    utils::assert_succeeds("int main(int argc, char *argv[]) {}");
    // TODO: check char main[] is valid
}

#[test]
fn declare_local() {
    utils::assert_succeeds(
        "int main(void) {
    int i = 0;
    return i;
}",
    );
}

#[test]
fn declare_local_function() {
    utils::assert_code(
        "
int main() {
    int f();
    return f();
}

int f() { return 1; }",
        1,
    );
    // make sure f goes out of scope
    utils::assert_compile_error(
        "
int main() {
    int f();
    return f();
}

int g() {
    return f();
}",
    );
}

#[test]
fn infer_array_bounds() {
    utils::assert_succeeds("int a[] = {1, 2, 3}; int main(){}");
    utils::assert_compile_error("int a[][] = {{1}};");
    utils::assert_succeeds("int a[][3] = {{1, 2, 3}}; int main(){}");
    utils::assert_compile_error("int a[][1] = {{1, 2, 3}}; int main(){}");
}

#[test]
fn declare_global() {
    utils::assert_succeeds("int x; int main() { return x; }");
}

#[test]
fn typedef() {
    utils::assert_succeeds(
        "
    typedef int i;
    i main() {}
",
    );
    utils::assert_code(
        "typedef int DWORD, INT, *INT_POINTER;
    INT main() {
        DWORD i = 1;
        INT_POINTER p = &i;
        return *p;
    }",
        1,
    );
    utils::assert_code(
        "typedef int INT; typedef INT i;
         i main() { return 1; }",
        1,
    );
    utils::assert_succeeds(
        "int main() {
    typedef void v;
}",
    );
    utils::assert_compiles_no_main("typedef struct __IO_FILE FILE;");
    utils::assert_succeeds(
        "
    typedef unsigned char UC;
    union {
            UC UC[1];
    } data;
    int main () {
        return data.UC[0];
    }",
    );
    utils::assert_compiles_no_main("long const long I; // should declare `l` as a long long");
    utils::assert_compile_error("typedef long L;
    int main() {
            long long ll;  // should declare ll as a long long
            L L ll2; // should be an error, declares `L` as a long and then there needs to be a comma between L and ll2
    }");
    utils::assert_compile_error(
        "typedef int I;
    I I; // should be an error, compare `int int;`",
    );
    utils::assert_succeeds(
        "typedef int I;
    int main() {
        I I; // should declare a variable of type int with name 'I'
        return I = 0;
    }",
    );
}

#[test]
fn struct_and_enum() {
    utils::assert_compile_error("enum e;");
    utils::assert_compile_error("enum;");
    utils::assert_compile_error("union;");
    utils::assert_compile_error("struct;");
}

#[test]
fn enum_self_assign() {
    utils::assert_code(
        "enum e {
        A = 1,
        B = A,
        C = B + 1,
    };
    int main() {
        return B;
    }",
        1,
    );
}

#[test]
fn alignment() {
    utils::assert_code(
        "struct s { int a[24]; } my_s;
         int main() { return *my_s.a; }",
        0,
    );
    utils::assert_succeeds(
        "struct s{int i,c,w;};
         union u{struct s _;int _;}i;
         int main() {}",
    );
}

#[test]
fn function() {
    utils::assert_code(
        "
        int f(int i) { return i; }
        int main() { return f(1); }
    ",
        1,
    );
    utils::assert_code(
        "
        int f(double d) { return d + .5; }
        int main() { return f(1.2); }
    ",
        1,
    );
    utils::assert_code(
        "
        int f(char c) { return c + .5; }
        int main(void) { return f(1.2); }
    ",
        1,
    );
    utils::assert_compile_error("int f(void, void);");
    utils::assert_compile_error("int f(int, void);");
    utils::assert_compile_error("int f(..., void);");
}

#[test]
fn forward_declaration() {
    // declaration of a struct without a usage is allowed
    // TODO: warn that this does nothing
    utils::assert_succeeds("extern struct s my_s; int main(){}");
    utils::assert_compile_error("struct s { struct t t; };");
    utils::assert_succeeds(
        "
    typedef struct s *sp;
    struct s { int i; } my_s;
    int main() {
        sp my_p = &my_s;
        return my_p->i = 0;
    }",
    );
    utils::assert_compile_error("union u my_u = my_u;");
    utils::assert_compile_error("struct u my_u = my_u;");
}

#[test]
fn recursive_struct() {
    utils::assert_code(
        "struct p {
        int i;
        struct p *q;
    } my_p;
    int main() {
        my_p.q = &my_p;
        my_p.q->q->q->i = 1;
        return my_p.i;
    }
    ",
        1,
    );
}

#[test]
#[ignore]
fn forward_struct_declaration() {
    utils::assert_succeeds(
        "struct s my_s;
    struct s {
        int i;
    };
    int main() {
        return my_s.i;
    }",
    );
}

#[test]
fn useless_declaration() {
    utils::assert_succeeds(
        "struct s {
            struct inner {
                int i;
            };
            int j;
        };
        struct inner my_inner;
        int main() {
            return my_inner.i;
        }",
    );
}

#[test]
fn scope() {
    utils::assert_succeeds(
        "struct T { int x; };
int main() {
        struct T { int z; };
}",
    );
}

#[test]
fn redefinition() {
    utils::assert_compile_error("enum e { A }; enum e { A };");
    utils::assert_compile_error("struct s { int i; }; struct s { int i; };");
    utils::assert_compile_error("union u { int i; }; union u { int i; };");
}

#[test]
fn linkage() {
    utils::assert_code("extern int a[] = {1, 2, 3}; int main() { return *a; }", 1);
    utils::assert_succeeds("extern int a[]; int main() {}");
    utils::assert_succeeds("extern int main(); extern int main() {}; extern int main();");
    utils::assert_succeeds("static int f() { return 1; } int main() {}");
    utils::assert_succeeds("extern int a[]; int a[] = {0,1,2}; int main() { return a[0]; }");
    utils::assert_succeeds("int a[] = {0,1,2}; extern int a[]; int main() { return a[0]; }");
}

#[test]
fn syntax() {
    utils::assert_compile_error("union int w=");
}
