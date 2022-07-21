#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

extern "C" {

char *make_jwt(const char *secretcstring, const int64_t *timestamp);

} // extern "C"
