import Navbar from './components/Navbar';
import Hero from './components/Hero';
import Features from './components/Features';
import QuoteCalculator from './components/QuoteCalculator';
import AnchorSimulator from './components/AnchorSimulator';
import Testimonials from './components/Testimonials';
import Footer from './components/Footer';

function App() {
  return (
    <>
      {/* Low-opacity background logo watermark */}
      <div style={{
        position: 'fixed',
        top: '50%',
        left: '50%',
        transform: 'translate(-50%, -50%)',
        width: '60vw',
        height: '60vh',
        backgroundImage: 'url(/logo.png)',
        backgroundPosition: 'center',
        backgroundRepeat: 'no-repeat',
        backgroundSize: 'contain',
        opacity: 0.02,
        pointerEvents: 'none',
        zIndex: -1
      }} />

      <Navbar />
      <main style={{ position: 'relative', zIndex: 1 }}>
        <Hero />
        <Features />
        {/*<QuoteCalculator />*/}
        {/*<AnchorSimulator />*/}
        <Testimonials />
      </main>
      <Footer />
    </>
  );
}

export default App;
