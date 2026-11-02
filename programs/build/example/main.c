#include "greet.h"

int main(int argc, char **argv) {
    greet(argc > 1 ? argv[1] : "world");
    return 0;
}
