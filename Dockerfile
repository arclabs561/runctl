FROM python:3.9-slim
WORKDIR /app
COPY training/ /app/
RUN pip install --no-cache-dir -q numpy
CMD ["python3", "train_mnist_e2e.py", "--epochs", "2"]
