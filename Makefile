
# call cmake with the correct arguments: -DCMAKE_BUILD_TYPE=Release

all:
	cmake -S . -DCMAKE_BUILD_TYPE=Release
	cmake --build . --config Release
