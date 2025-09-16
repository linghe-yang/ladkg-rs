# Cryptocurrency Price Dataset
Due to Github's limit on sharing files larger than 100 MB, we are making this file available on Google drive [here](https://drive.google.com/file/d/1uK8TAVtGVBbaWY7qVND63E_EKm0r0r2q/view?usp=sharing). We thank the staff at [SupraOracles](https://www.supra.com) for collecting raw data from these exchanges for these cryptocurrencies. This dataset contains the price exchange data of 63 prominent cryptocurrencies including Bitcoin, Ethereum, Dogecoin, and Algorand from 7th July to 21st July. The readings have been collected at a frequency of one reading every two minutes. The readings have been collected from the following exchanges: `Binance`, `Coinbase`, `Crypto.com`, `Gate.io`, `Huobi`, `Mexc`, `Poloniex`, `Bybit`, `Kucoin`, `okex` and `Kraken`. The values of the cryptocurrency have been reported as the corresponding equivalent of `USDT`, a digital currency pegged to the US Dollar. The data is present in the form of a JSON file with the following format. 
```
{
    "btc_usdt": {
        "1688737482000": {
            "bybit": 30250.2,
            "poloniex": 30269.120000000003,
            "okex": 30269.3,
            "huobi_global": 30270.999999999996,
            "coinbase_pro": 30271.81,
            "gateio": 30272.4,
            "mexc": 30273.7,
            "binance": 30273.7,
            "kraken": 30273.7,
            "kucoin": 30273.8,
            "binance_us": 30289.989999999998
        },
        ...
    },
    "eth_usdt": {
        "1688737257000": {
            "binance_us": 1864.84,
            "bybit": 1866,
            "poloniex": 1866.8999999999999,
            "huobi_global": 1867,
            "gateio": 1867.16,
            "mexc": 1867.16,
            "binance": 1867.16,
            "okex": 1867.23,
            "kucoin": 1867.4,
            "coinbase_pro": 1867.48
        },
        ...
    },
    ...
}
```