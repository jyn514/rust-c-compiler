// code: 6
int puts(const char *s);
int putchar(char);

typedef struct s *sp;

int i = 1;
int a[3] = {1, 2, 3};
float f = 2.5;

struct s {
  int outer;
} my_struct;

int g(int);

int main(void) {
  sp my_struct_pointer = &my_struct;
  const int c = my_struct_pointer->outer = 4;
  // should return 6
  int j = i + f*a[2] - c/g(1);
  puts("i is ");
  putchar('0' + j);
  putchar('\n');
  return j;
}

int g(int i) {
  if (i < 0 || i >= 3) {
    return 0;
  }
  return a[i];
}
