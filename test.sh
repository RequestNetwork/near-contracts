cd mocks/
./build.sh
cd ..
./build.sh
echo "-- UNIT TESTS --"
cargo test -p conversion_proxy
cargo test -p fungible_conversion_proxy
cargo test -p fungible_proxy
echo "-- SANITY TESTS --"
cargo test -p mocks
echo "-- SIMULATED TESTS --"
cargo test