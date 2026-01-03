#include <stdio.h>

int add(int a, int b);
int sub(int a, int b);

int main(void) {
    printf("calc: %d %d\n", add(2, 2), sub(5, 3));
    return 0;
}
