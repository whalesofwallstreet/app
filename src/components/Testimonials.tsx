import React from 'react';
import { motion } from 'framer-motion';

const testimonials = [
  {
    text: "I highly recommend this app. Seamlessly bridging across chains to Stellar has never been easier or faster.",
    author: "Ifeanyichukwu Obaji",
    avatar: "https://images.unsplash.com/photo-1534528741775-53994a69daeb?auto=format&fit=crop&w=150&h=150&q=80"
  },
  {
    text: "Wow app completely changed our treasury offramping. The stellar anchor network integrations just work perfectly in the background.",
    author: "Babajide Duroshola",
    avatar: "https://images.unsplash.com/photo-1507003211169-0a1dd7228f2d?auto=format&fit=crop&w=150&h=150&q=80"
  },
  {
    text: "The best user interface for Stellar conversion. Direct integration with multi-chain wallets is beautifully simple.",
    author: "@RealSOK_",
    avatar: "https://images.unsplash.com/photo-1492562080023-ab3db95bfbce?auto=format&fit=crop&w=150&h=150&q=80"
  }
];

const Testimonials: React.FC = () => {
  return (
    <section style={{ padding: '6rem 0', backgroundColor: 'var(--bg-light)', overflow: 'hidden' }}>
      <div className="container">
        <motion.h2 
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.6 }}
          className="section-title"
          style={{ color: 'var(--primary-color)' }}
        >
          Don't just take our word for it
        </motion.h2>

        <div style={{ 
          display: 'grid', 
          gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', 
          gap: '2rem', 
          marginTop: '3rem' 
        }}>
          {testimonials.map((t, idx) => (
            <motion.div 
              key={idx}
              initial={{ opacity: 0, y: 30 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.5, delay: idx * 0.1 }}
              whileHover={{ y: -8, boxShadow: 'var(--shadow-lg)' }}
              style={{
                backgroundColor: 'var(--white)',
                padding: '2.5rem 2rem',
                borderRadius: '1.5rem',
                boxShadow: 'var(--shadow-md)',
                display: 'flex',
                flexDirection: 'column',
                justifyContent: 'space-between',
                border: '1px solid rgba(51, 51, 160, 0.04)'
              }}
            >
              <p style={{ color: 'var(--text-gray)', lineHeight: 1.6, marginBottom: '2rem', fontSize: '1rem' }}>
                "{t.text}"
              </p>
              
              <div style={{ display: 'flex', alignItems: 'center', gap: '1rem' }}>
                <img 
                  src={t.avatar} 
                  alt={t.author} 
                  style={{
                    width: '44px',
                    height: '44px',
                    borderRadius: '50%',
                    objectFit: 'cover',
                    border: '2px solid var(--secondary-color)'
                  }}
                />
                <div style={{ fontWeight: 700, color: 'var(--text-dark)' }}>
                  {t.author}
                </div>
              </div>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
};

export default Testimonials;
