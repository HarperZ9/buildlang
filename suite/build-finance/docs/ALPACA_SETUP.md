# Setting Up Alpaca Paper Trading

## Step 1: Create Alpaca Account
1. Go to https://app.alpaca.markets/signup
2. Create account (paper trading is free, no funding required)
3. Complete identity verification

## Step 2: Get API Credentials
1. Log in to https://app.alpaca.markets
2. Go to "Paper Trading" (not Live)
3. Click "View API Keys" or navigate to API Keys page
4. Generate a new API key pair
5. Save both:
   - **API Key ID** (e.g., `PKXXXXXXXXXXXXXXXX`)
   - **API Secret Key** (e.g., `xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx`)

## Step 3: Configure build-finance

### Via CLI
```bash
build-finance backtest --broker alpaca \
    --api-key PKXXXXXXXXXXXXXXXX \
    --api-secret xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx \
    --symbol AAPL --strategy momentum
```

### Via Python API
```python
from build_finance.broker import AlpacaBroker, BrokerConfig

config = BrokerConfig(
    name="alpaca",
    api_key="PKXXXXXXXXXXXXXXXX",
    api_secret="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    paper_trading=True,  # ALWAYS use paper first
)

broker = AlpacaBroker(config)
account = broker.get_account()
print(f"Equity: ${account.equity:.2f}")
```

### Via GUI
1. Launch: `build-finance gui`
2. Go to Settings page
3. Enter API Key and API Secret
4. Click "Test Connection"

## Step 4: Verify Connection
```python
from build_finance.market_data import fetch_yahoo

candles = fetch_yahoo("AAPL", period="5d", interval="1d")
print(f"Got {len(candles)} candles")
```

## Important Notes
- **ALWAYS test with paper trading first**
- Paper trading uses real market data but fake money ($100,000 default)
- Live trading requires real funding and carries real financial risk
- API keys should never be committed to version control
