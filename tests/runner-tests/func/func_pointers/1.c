// code: 1

int f() { return 1; }
int main() {
    int (*func)() = f;
    return (*func)();
}
