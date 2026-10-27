#include "terrain.hpp"

namespace game {
Terrain::Terrain() : count(0) {}
Terrain::~Terrain() = default;

void Terrain::update(int value) {
    count = twice(value);
}

int twice(int value) {
    return value * 2;
}
}
