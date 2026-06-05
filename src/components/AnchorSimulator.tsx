import { useState } from 'react';

export default function AnchorSimulator() {
  const [anchorDomain, setAnchorDomain] = useState('testanchor.stellar.org');
  const [assetCode, setAssetCode] = useState('USDC');
  const [account, setAccount] = useState('GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA');
  const [loading, setLoading] = useState(false);
  const [simType, setSimType] = useState<'deposit' | 'withdraw'>('deposit');
  const [simResult, setSimResult] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);

  const handleSimulate = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    setSimResult(null);

    const endpoint = simType === 'deposit' 
      ? 'http://127.0.0.1:8080/api/v1/anchor/deposit'
      : 'http://127.0.0.1:8080/api/v1/anchor/withdraw';

    try {
      const response = await fetch(endpoint, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          anchor_domain: anchorDomain,
          asset_code: assetCode,
          account: account,
        }),
      });

      if (!response.ok) {
        const errData = await response.json();
        throw new Error(errData.error || 'Failed to trigger simulator');
      }

      const data = await response.json();
      setSimResult(data);
    } catch (err: any) {
      setError(err.message || 'Simulation error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <section id="anchors" className="container" style={{ padding: '4rem 1.5rem' }}>
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
        <h2 className="section-title text-gradient" style={{ marginBottom: '1rem' }}>
          Stellar Anchor Flow Simulator
        </h2>
        <p style={{ textAlign: 'center', color: 'var(--text-gray)', marginBottom: '2.5rem' }}>
          Test interactive SEP-24 customer onboarding flows against simulated Stellar Anchors.
        </p>

        <div style={{ display: 'flex', gap: '1rem', marginBottom: '2rem', justifyContent: 'center' }}>
          <button 
            onClick={() => setSimType('deposit')}
            style={{
              padding: '0.6rem 1.5rem',
              borderRadius: '9999px',
              fontWeight: 700,
              border: '2px solid var(--primary-color)',
              background: simType === 'deposit' ? 'var(--primary-color)' : 'transparent',
              color: simType === 'deposit' ? 'var(--white)' : 'var(--primary-color)',
              transition: 'var(--transition)'
            }}
          >
            Deposit (On-Ramp)
          </button>
          <button 
            onClick={() => setSimType('withdraw')}
            style={{
              padding: '0.6rem 1.5rem',
              borderRadius: '9999px',
              fontWeight: 700,
              border: '2px solid var(--primary-color)',
              background: simType === 'withdraw' ? 'var(--primary-color)' : 'transparent',
              color: simType === 'withdraw' ? 'var(--white)' : 'var(--primary-color)',
              transition: 'var(--transition)'
            }}
          >
            Withdraw (Off-Ramp)
          </button>
        </div>

        <form onSubmit={handleSimulate} style={{ display: 'flex', flexDirection: 'column', gap: '1.5rem' }}>
          <div>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 600 }}>Anchor Domain</label>
            <input 
              type="text" 
              value={anchorDomain} 
              onChange={(e) => setAnchorDomain(e.target.value)}
              className="glass-input"
            />
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 600 }}>Asset Code</label>
            <input 
              type="text" 
              value={assetCode} 
              onChange={(e) => setAssetCode(e.target.value)}
              className="glass-input"
            />
          </div>
          <div>
            <label style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 600 }}>Stellar Account Address</label>
            <input 
              type="text" 
              value={account} 
              onChange={(e) => setAccount(e.target.value)}
              className="glass-input"
            />
          </div>
          <button 
            type="submit" 
            className="btn btn-primary" 
            style={{ padding: '1rem', marginTop: '0.5rem' }}
            disabled={loading}
          >
            {loading ? 'Initializing Flow...' : `Initiate Simulated ${simType === 'deposit' ? 'Deposit' : 'Withdrawal'}`}
          </button>
        </form>

        {error && (
          <div style={{ marginTop: '2rem', padding: '1rem', background: '#ffeef0', border: '1px solid #ffccd3', borderRadius: '0.5rem', color: '#b32434' }}>
            <strong>Error:</strong> {error}
          </div>
        )}

        {simResult && (
          <div style={{ marginTop: '2.5rem', padding: '1.5rem', borderRadius: '1rem', background: 'rgba(43, 43, 145, 0.05)', border: '1px solid var(--glass-border)' }}>
            <h3 style={{ fontSize: '1.2rem', marginBottom: '0.75rem', fontWeight: 700 }}>Interactive Flow Link Generated</h3>
            <p style={{ fontSize: '0.9rem', color: 'var(--text-gray)', marginBottom: '1rem' }}>
              Transaction ID: <code style={{ color: 'var(--primary-color)' }}>{simResult.id}</code>
            </p>
            <a 
              href={simResult.url} 
              target="_blank" 
              rel="noopener noreferrer"
              className="btn btn-gradient"
              style={{ textDecoration: 'none', display: 'inline-flex', padding: '0.8rem 1.5rem' }}
            >
              Open Interactive Window
            </a>
          </div>
        )}
      </div>
    </section>
  );
}
