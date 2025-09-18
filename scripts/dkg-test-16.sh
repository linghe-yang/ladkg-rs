# A script to test quickly

killall {node} &> /dev/null
rm -rf /tmp/*.db &> /dev/null
tri=1000000

rand=$(shuf -i 1000-150000000 -n 1)
TESTDIR=${TESTDIR:="testdata/hyb_16"}
TYPE=${TYPE:="release"}
EXP=${EXP:-"appxcox_new"}
W=${W:="10000"}
curr_date=$(date +"%s%3N")
sleep=1
st_time=$((curr_date+sleep))
echo $st_time
# Run the syncer now
./target/$TYPE/node \
    --config $TESTDIR/nodes-0.json \
    --ip ip_file \
    --sleep $st_time \
    --vsstype sync \
    --syncer $1 \
    --batch 10 \
    --rand $rand 2> logs/syncer.log &
for((i=0;i<16;i++)); do
./target/$TYPE/node \
    --config $TESTDIR/nodes-$i.json \
    --ip ip_file \
    --sleep $st_time \
    --vsstype dkg \
    --syncer $1 \
    --batch 10 \
    --rand $rand 2> logs/$i.log &
done

# Kill all nodes sudo lsof -ti:7000-7015 | xargs kill -9