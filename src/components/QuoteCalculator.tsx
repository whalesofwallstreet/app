import { useState } from 'react';

export default function QuoteCalculator() {
  const [sourceChain, setSourceChain] = useState('Ethereum');
  const [destChain, setDestChain] = useState('Solana');
  const [sourceAsset, setSourceAsset] = useState('USDC');
  const [destAsset, setDestAsset] = useState('USDC');
  const [amountIn, setAmountIn] = useState('1000');
  const [routes, setRoutes] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleCalculate = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const response = await fetch('http://127.0.0.1:8080/api/v1/quote', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          source_chain: sourceChain,
          dest_chain: destChain,
          source_asset: sourceAsset,
          dest_asset: destAsset,
          amount_in: parseInt(amountIn, 10),
        }),
      });

      if (!response.ok) {
        throw new Error(await response.text());
      }

      const data = await response.json();
      setRoutes(data.routes || []);
    } catch (err: any) {
      setError(err.message || 'Failed to fetch quote');
    } finally {
      setLoading(false);
    }
  };

  return (
    <section id="calculator" className="container" style={{ padding: '4rem 1.5rem' }}>
      <div style={{
        background: 'var(--glass-bg)',
        backdropFilter: 'blur(12px)',
        border: '1px solid var(--glass-border)',
        borderRadius: '1.5rem',
        padding: '3rem',
        boxShadow: 'var(--shadow-lg)',
        maxWidth: '800px',
        margin: '0 auto'
      }}>
        <h2 className="section-title text-gradient" style={{ marginBottom: '2rem' }}>
          Interactive Quoting Engine
        </h2>
        <form onSubmit={handleCalculate} style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1.5rem' }}>
          <div>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 600 }}>Source Chain</label>
            <select 
              value={sourceChain} 
              onChange={(e) => setSourceChain(e.target.value)}
              className="glass-input"
            >
              <option value="Ethereum">Ethereum</option>
              <option value="Arbitrum">Arbitrum</option>
              <option value="Solana">Solana</option>
              <option value="Stellar">Stellar</option>
            </select>
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 600 }}>Destination Chain</label>
            <select 
              value={destChain} 
              onChange={(e) => setDestChain(e.target.value)}
              className="glass-input"
            >
              <option value="Ethereum">Ethereum</option>
              <option value="Arbitrum">Arbitrum</option>
              <option value="Solana">Solana</option>
              <option value="Stellar">Stellar</option>
            </select>
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 600 }}>Source Asset</label>
            <input 
              type="text" 
              value={sourceAsset} 
              onChange={(e) => setSourceAsset(e.target.value)}
              className="glass-input"
            />
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 600 }}>Destination Asset</label>
            <input 
              type="text" 
              value={destAsset} 
              onChange={(e) => setDestAsset(e.target.value)}
              className="glass-input"
            />
          </div>
          <div style={{ gridColumn: 'span 2' }}>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 600 }}>Amount In</label>
            <input 
              type="number" 
              value={amountIn} 
              onChange={(e) => setAmountIn(e.target.value)}
              className="glass-input"
            />
          </div>
          <button 
            type="submit" 
            className="btn btn-primary" 
            style={{ gridColumn: 'span 2', padding: '1rem', marginTop: '0.5rem' }}
            disabled={loading}
          >
            {loading ? 'Finding Best Route...' : 'Calculate Routes'}
          </button>
        </form>

        {error && (
          <div style={{ marginTop: '2rem', padding: '1rem', background: '#ffeef0', border: '1px solid #ffccd3', borderRadius: '0.5rem', color: '#b32434' }}>
            {error}
          </div>
        )}

        {routes.length > 0 && (
          <div style={{ marginTop: '2.5rem' }}>
            <h3 style={{ fontSize: '1.25rem', marginBottom: '1rem', fontWeight: 700 }}>Available Routes</h3>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
              {routes.map((route, i) => (
                <div key={i} style={{
                  padding: '1.25rem',
                  borderRadius: '0.75rem',
                  background: i === 0 ? 'rgba(22, 140, 107, 0.08)' : 'rgba(0, 0, 0, 0.02)',
                  border: i === 0 ? '2px solid var(--secondary-color)' : '1px solid var(--glass-border)',
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center'
                }}>
                  <div>
                    <span style={{ fontWeight: 700, fontSize: '1.1rem', color: i === 0 ? 'var(--secondary-color)' : 'var(--text-dark)' }}>
                      {route.provider}
                    </span>
                    <span style={{ fontSize: '0.85rem', color: 'var(--text-gray)', marginLeft: '1rem' }}>
                      {route.path}
                    </span>
                  </div>
                  <div style={{ textAlign: 'right' }}>
                    <div style={{ fontWeight: 800, fontSize: '1.1rem' }}>
                      {route.amount_out} Out
                    </div>
                    <div style={{ fontSize: '0.8rem', color: 'var(--text-gray)' }}>
                      Fee: ${route.estimated_fee_usd} | {route.duration_seconds}s
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
