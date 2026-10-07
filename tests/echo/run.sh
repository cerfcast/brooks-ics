#!/bin/bash

# brooks-ics, Copyright 2026, Will Hawkins
#
# This file is part of brooks-ics.

# This file is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program. If not, see <https:#www.gnu.org/licenses/>.

for i in 1 2; do
				expected_matches=$(cat $i.check | wc -l)
				actual_matches=$(source $i.curl 2>&1 | grep -cf $i.check)
				if [ $expected_matches != $actual_matches ]; then
								echo "Test $i failed: Expected $expected_matches and found $actual_matches"
								exit 1
				fi
done
echo "All tests passed."
