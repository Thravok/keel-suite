#include "RpcClient.h"

#include <QJsonDocument>
#include <QNetworkRequest>

RpcClient::RpcClient(QObject *parent) : QObject(parent) {
  connect(&ws_, &QWebSocket::connected, this, &RpcClient::onConnected);
  connect(&ws_, &QWebSocket::textMessageReceived, this, &RpcClient::onTextMessage);
  connect(&ws_, &QWebSocket::disconnected, this, [this] { emit connectedChanged(false); });
}

void RpcClient::connectTo(quint16 port) {
  QUrl url(QStringLiteral("ws://127.0.0.1:%1").arg(port));
  QNetworkRequest req(url);
  req.setRawHeader("Origin", "keel://");
  if (const auto token = qgetenv("KEEL_LIGHT_RPC_TOKEN"); !token.isEmpty())
    req.setRawHeader("Authorization", QByteArrayLiteral("Bearer ") + token);
  ws_.open(req);
}

void RpcClient::onConnected() { emit connectedChanged(true); }

void RpcClient::call(const QString &method, const QJsonObject &params,
                     std::function<void(QJsonValue)> ok,
                     std::function<void(QString)> err) {
  const int id = nextId_++;
  pending_.insert(id, Pending{std::move(ok), std::move(err)});
  QJsonObject msg{{"jsonrpc", "2.0"}, {"id", id}, {"method", method}, {"params", params}};
  ws_.sendTextMessage(QString::fromUtf8(QJsonDocument(msg).toJson(QJsonDocument::Compact)));
}

void RpcClient::onTextMessage(const QString &msg) {
  const auto doc = QJsonDocument::fromJson(msg.toUtf8());
  if (!doc.isObject())
    return;
  const auto obj = doc.object();
  const int id = obj.value("id").toInt();
  if (!pending_.contains(id))
    return;
  auto p = pending_.take(id);
  if (obj.contains("error")) {
    if (p.err)
      p.err(obj.value("error").toObject().value("message").toString());
    return;
  }
  if (p.ok)
    p.ok(obj.value("result"));
}
