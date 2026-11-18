// The MSD oracle: Etterna's own MinaCalc, built by itself (build.sh), fed
// rows on stdin and printing its eight numbers, for `ddi-msd`'s opt-in
// comparison (`crates/msd/tests/oracle.rs`). Nothing of the game is linked.
//
// Input, one request after another:
//   chart <id> <rate f32 bits hex> <goal f32 bits hex> <ssr 0|1> <rows>
//   <notes> <time f32 bits hex>          (one line a row, `rows` of them)
//   fastsqrt <count>
//   <x f32 bits hex>                      (one line a value)
// Output: `<id> <eight f32 bits hex>` a chart, `<result bits hex>` a value.
// Each chart is rated by a fresh Calc (see crates/msd/README.md).

#include "MinaCalc.h"
#include "MinaCalcHelpers.h"
#include "PatternModHelpers.h"

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <iostream>
#include <memory>
#include <string>
#include <vector>

static auto
from_bits(uint32_t b) -> float
{
	float f;
	std::memcpy(&f, &b, sizeof f);
	return f;
}

static auto
to_bits(float f) -> uint32_t
{
	uint32_t b;
	std::memcpy(&b, &f, sizeof b);
	return b;
}

int
main()
{
	std::string word;
	while (std::cin >> word) {
		if (word == "chart") {
			std::string id;
			uint32_t rate_bits = 0;
			uint32_t goal_bits = 0;
			int ssr = 0;
			size_t rows = 0;
			std::cin >> id >> std::hex >> rate_bits >> goal_bits >> std::dec >>
			  ssr >> rows;
			std::vector<NoteInfo> ni(rows);
			for (auto& r : ni) {
				uint32_t time_bits = 0;
				std::cin >> std::dec >> r.notes >> std::hex >> time_bits >>
				  std::dec;
				r.rowTime = from_bits(time_bits);
			}
			auto calc = std::make_unique<Calc>();
			std::vector<float> out;
			if (ni.size() <= 1) {
				out = dimples_the_all_zero_output;
			} else {
				calc->ssr = ssr != 0;
				calc->debugmode = false;
				calc->keycount = 4;
				const auto goal = from_bits(goal_bits);
				out = calc->CalcMain(
				  ni, from_bits(rate_bits), ssr != 0 ? std::min(goal, ssr_goal_cap) : goal);
			}
			std::printf("%s", id.c_str());
			for (auto v : out) {
				std::printf(" %08x", to_bits(v));
			}
			std::printf("\n");
		} else if (word == "fastsqrt") {
			size_t count = 0;
			std::cin >> std::dec >> count;
			for (size_t i = 0; i < count; ++i) {
				uint32_t x = 0;
				std::cin >> std::hex >> x >> std::dec;
				std::printf("%08x\n", to_bits(fastsqrt(from_bits(x))));
			}
		} else if (word == "version") {
			std::printf("%d\n", GetCalcVersion());
		} else {
			std::fprintf(stderr, "unknown request %s\n", word.c_str());
			return 2;
		}
		std::fflush(stdout);
	}
	return 0;
}
