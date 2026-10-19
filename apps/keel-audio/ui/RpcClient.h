#pragma once

#include <QObject>
#include <QHash>
#include <QJsonObject>
#include <QJsonValue>
#include <QWebSocket>
#include <functional>

class RpcClient : public QObject {
  Q_OBJECT
public:
  explicit RpcClient(QObject *parent = nullptr);
  void connectTo(quint16 port);
  void call(const QString &method, const QJsonObject &params,
            std::function<void(QJsonValue)> ok,
            std::function<void(QString)> err = {});

signals:
  void connectedChanged(bool on);

private slots:
  void onConnected();
  void onTextMessage(const QString &msg);

private:
  QWebSocket ws_;
  int nextId_ = 1;
  struct Pending {
    std::function<void(QJsonValue)> ok;
    std::function<void(QString)> err;
  };
  QHash<int, Pending> pending_;
};
