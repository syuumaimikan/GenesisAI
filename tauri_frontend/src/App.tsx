// tauri_frontend/src/App.tsx
import React, { useState, useRef, useEffect } from "react";
import "./App.css";

interface Message {
  id: string;
  text: string;
  sender: "user" | "bot";
}

const App: React.FC = () => {
  const [messages, setMessages] = useState<Message[]>([
    {
      id: "welcome",
      text: "こんにちは！私はGenesisAIです。React + TypeScript + Material You インターフェースへようこそ。どのような監査やPC自動化を実行しますか？",
      sender: "bot",
    },
  ]);
  const [input, setInput] = useState("");
  const chatLogRef = useRef<HTMLDivElement>(null);

  // 新しいメッセージ受信時に最下部へ自動スクロール
  useEffect(() => {
    if (chatLogRef.current) {
      chatLogRef.current.scrollTop = chatLogRef.current.scrollHeight;
    }
  }, [messages]);

  const handleSend = async () => {
    if (!input.trim()) return;

    const userMsg: Message = {
      id: Date.now().toString(),
      text: input,
      sender: "user",
    };

    setMessages((prev) => [...prev, userMsg]);
    const currentInput = input;
    setInput("");

    try {
      const response = await fetch("http://127.0.0.1:8080/api/chat", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ message: currentInput }),
      });

      if (!response.ok) throw new Error("API接続エラー");
      const data = await response.json();

      const botMsg: Message = {
        id: (Date.now() + 1).toString(),
        text: data.response || "応答の生成に失敗しました。",
        sender: "bot",
      };
      setMessages((prev) => [...prev, botMsg]);
    } catch (error) {
      const errMsg: Message = {
        id: (Date.now() + 1).toString(),
        text: "❌ バックエンドサーバーに接続できません。cargo run -p genesis_main を起動してください。",
        sender: "bot",
      };
      setMessages((prev) => [...prev, errMsg]);
    }
  };

  const handleKeyPress = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      handleSend();
    }
  };

  return (
    <div className="md-container">
      {/* AppBar (Header) */}
      <div className="md-appbar">
        <div className="md-avatar">G</div>
        <div>
          <div className="md-title">GenesisAI</div>
          <div className="md-subtitle">自律OSエージェント (React + TS)</div>
        </div>
      </div>

      {/* Message Area */}
      <div className="md-chatlog" ref={chatLogRef}>
        {messages.map((msg) => (
          <div key={msg.id} className={`md-bubble ${msg.sender}`}>
            {msg.text}
          </div>
        ))}
      </div>

      {/* Input Bar */}
      <div className="md-input-bar">
        <input
          type="text"
          className="md-textfield"
          placeholder="メッセージを入力..."
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyPress={handleKeyPress}
        />
        <button className="md-fab" onClick={handleSend}>
          <svg viewBox="0 0 24 24">
            <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
          </svg>
        </button>
      </div>
    </div>
  );
};

export default App;
