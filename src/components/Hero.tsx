import React from 'react';
import { motion } from 'framer-motion';
import { ChevronRight } from 'lucide-react';
import phoneMockup from '../assets/wow-phone-mockup.png';

const Hero: React.FC = () => {
  return (
    <section style={{ overflow: 'hidden', position: 'relative' }}>
      <div className="container">
        <div className="hero-wrapper">

          <motion.div
            initial={{ opacity: 0, y: 30 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.8, ease: [0.16, 1, 0.3, 1] }}
            className="hero-content"
          >
            <h1 style={{
              fontSize: 'clamp(1.5rem, 5vw, 3rem)',
              lineHeight: 1.1,
              fontWeight: 800,
              marginBottom: '1.5rem',
              color: 'var(--text-dark)'
            }}>
              Your everything app
            </h1>
            <p style={{
              fontSize: 'clamp(1.1rem, 2vw, 1.25rem)',
              color: 'var(--text-gray)',
              marginBottom: '2.5rem',
              lineHeight: 1.6
            }}>
              The Wow app is a fully-featured digital banking experience built for modern finance. Send money, save towards your goals and manage your spending all in one sleek interface.
              <br /><br />
              <strong style={{ color: 'var(--text-dark)' }}>The Wow app is powered by the Wow Engine.</strong>
            </p>

            <button className="btn btn-gradient" style={{
              fontSize: '1.125rem',
              display: 'inline-flex',
              alignItems: 'center',
              gap: '0.75rem'
            }}>
              Download WOW app <ChevronRight size={20} />
            </button>
          </motion.div>

          <motion.div
            initial={{ opacity: 0, scale: 0.9, y: 50 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            transition={{ duration: 0.8, delay: 0.2, ease: [0.16, 1, 0.3, 1] }}
            className="hero-image-wrapper"
          >
            {/* Soft backdrop glow */}
            <div style={{
              position: 'absolute',
              width: '320px',
              height: '320px',
              borderRadius: '50%',
              background: 'var(--gradient-balance)',
              opacity: 0.15,
              filter: 'blur(60px)',
              zIndex: -1
            }}></div>

            <motion.img
              src={phoneMockup}
              alt="Wow App Mobile Demo"
              style={{
                maxWidth: '100%',
                height: 'auto',
                maxHeight: '520px',
                filter: 'drop-shadow(0 25px 40px rgba(51, 51, 160, 0.15))'
              }}
              animate={{ y: [-8, 8, -8] }}
              transition={{ repeat: Infinity, duration: 5, ease: "easeInOut" }}
            />
          </motion.div>

        </div>
      </div>
    </section>
  );
};

export default Hero;
