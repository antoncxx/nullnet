import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { StackProvider } from './StackContext';
import Dashboard from './pages/Dashboard';
import Services from './pages/Services';
import Nodes from './pages/Nodes';
import Sessions from './pages/Sessions';
import Config from './pages/Config';
import Events from './pages/Events';
import Certificates from './pages/Certificates';
import Topology from './pages/Topology';

export default function App() {
  return (
    <StackProvider>
      <BrowserRouter>
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/services" element={<Services />} />
          <Route path="/nodes" element={<Nodes />} />
          <Route path="/sessions" element={<Sessions />} />
          <Route path="/config" element={<Config />} />
          <Route path="/certificates" element={<Certificates />} />
          <Route path="/events" element={<Events />} />
          <Route path="/topology" element={<Topology />} />
        </Routes>
      </BrowserRouter>
    </StackProvider>
  );
}
