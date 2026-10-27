#pragma once
#include <vector>

namespace game {
struct Sample {
    float height;
};

enum class Mode { Near, Far };
using Samples = std::vector<Sample>;

template<class T>
class Buffer {
public:
    T value;
    T read() const { return value; }
};

class Terrain {
public:
    Terrain();
    ~Terrain();
    void update(int count);
    int size() const { return count; }
private:
    int count;
};
int twice(int value);
}
