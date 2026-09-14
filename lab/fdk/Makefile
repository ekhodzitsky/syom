# Isolated fdk-aac v2.0.3 adapter. Not invoked by cargo test.

FDK_PREFIX ?= $(error set FDK_PREFIX to an fdk-aac 2.0.3 install)

CC ?= gcc
CXX ?= c++
CFLAGS ?= -O2 -Wall -Wextra -std=c11
CPPFLAGS += -I$(FDK_PREFIX)/include
LDFLAGS += -L$(FDK_PREFIX)/lib
# Link with CXX: libfdk-aac is C++ (zig-c++ GNU / libc++ on this host).
LDLIBS += -lfdk-aac -lm -lpthread

.PHONY: all clean

all: fdk_driver

fdk_adapt.o: fdk_adapt.c fdk_adapt.h
	$(CC) $(CFLAGS) $(CPPFLAGS) -c fdk_adapt.c -o $@

fdk_driver.o: fdk_driver.c fdk_adapt.h
	$(CC) $(CFLAGS) $(CPPFLAGS) -c fdk_driver.c -o $@

fdk_driver: fdk_adapt.o fdk_driver.o
	$(CXX) $(CFLAGS) -o $@ fdk_adapt.o fdk_driver.o $(LDFLAGS) $(LDLIBS)

clean:
	rm -f fdk_adapt.o fdk_driver.o fdk_driver
