# Development 패키지 v1

이것은 BXDL 소유의 개발용 포맷이다. NIGO 공식 engine manifest와 공급자 release 계약은 미제공이며, 이 포맷이 그 계약을 대신하지 않는다. 입력 asset을 다운로드하거나 NIGO 소스를 빌드하지 않는다.

## Build 입력

검증해 선택한 JAR·대상 OS/CPU용 Java runtime·Rust CLI·배포 자료를 별도의 staging 폴더에 준비한다. Mac arm64를 우선하고 Linux amd64도 개발 profile로 유지한다. 모든 파일은 실제 regular file, 권한은 0644 또는 0755여야 한다. 심볼릭 링크·hardlink tar entry·장치·sparse/PAX/GNU extension은 허용하지 않는다. runtime 배포물에 링크가 있다면 신뢰한 빌드 준비 과정에서 실제 파일로 정리한 결과를 별도로 검토한다.

```text
stage/
  bin/bxdl                       # 필수, 0755
  engine/nigo-node.jar            # 필수
  runtime/bin/java                # 필수, 0755
  runtime/...                    # 실제 선정 runtime의 나머지 파일
  licenses/THIRD_PARTY_NOTICES    # 필수, 비어 있으면 거부
  licenses/SBOM.json              # 필수, 비어 있으면 거부
  docs/...                       # 선택
  deploy/...                     # 선택
  schemas/...                    # 선택
```

검증기는 필수 파일의 존재와 바이트 무결성을 검사하며 실제 JRE 구성·JAR 형식·NOTICE의 법적 충분성·SBOM 내용의 완전성을 보증하지 않는다. `SBOM.json` 내용 검증 및 공식 dependency 목록 대조는 실제 runtime/engine 공급 시 추가할 gate다. 개발 fixture의 가짜 JAR/Java bytes로 통과한 테스트를 엔진 실행으로 보고하지 않는다.

`packaging/package-spec.example.json`은 Mac용이며 `package-spec.linux.example.json`은 기존 Linux용이다. 별도 spec 파일로 복사하고 선택한 입력의 출처와 **예상 SHA-256**을 채운다. 예시의 placeholder는 실행 가능한 lock이 아니다. 신뢰하지 않은 파일의 hash를 계산하여 입력한 것만으로 출처가 인증되지는 않는다. Builder는 제공한 expected engine/Java hash와 staging 실제 bytes를 비교하고 자동으로 expected hash를 채우지 않는다.

```bash
bxdl package build --root ./stage --spec ./package-spec.json \
  --output ./bxdl-development.tar.gz --signing-key ./release-private.pem --json
```

서명은 Ed25519, private key는 unencrypted PKCS8 PEM, public key는 SPKI PEM 형식이다. key 생성·보관·권한·교체는 빌드 운영자가 관리한다. private key와 output은 stage 밖에 있어야 하며 key 원문은 출력하지 않는다. 이미 있는 output은 덮어쓰지 않는다. 입력 변경을 감지하거나 검증에 실패하면 자신이 새로 만든 output만 제거한다.

서명 없는 테스트 자료를 만들 때만 `--signing-key` 대신 `--allow-unsigned-development`를 명시한다. 정식 엔진/제품 납품 경로로 사용하지 않는다.

## 내부 순서와 서명

단일 gzip member 안의 USTAR archive다.

1. `manifest.json`: strict JSON, 필수 첫 regular entry.
2. `manifest.sig`: 서명 패키지에만 존재하는 두 번째 entry. 정확한 manifest 원문 bytes에 대한 Ed25519 서명을 base64로 인코딩한다.
3. manifest inventory와 정확히 일치하는 payload regular entries. Builder는 경로를 정렬한다.
4. 두 개의 zero EOF block. 추가 tar padding은 제한된 zero bytes만 허용한다.

Manifest 자체와 signature는 payload inventory에 포함하지 않는다. 각 payload의 path/size/SHA-256/mode를 manifest가 묶는다. gzip CRC·추가 member·EOF 이후 데이터까지 검사한다. 검증은 스트리밍 읽기만 수행하고 파일 추출·JVM·systemd·네트워크 작업을 하지 않는다.

허용 root는 `bin`, `engine`, `runtime`, `deploy`, `schemas`, `docs`, `licenses`다. 경로는 상대경로·정규형이어야 하며 절대경로·`..`·backslash·control character·중복 path·file/directory 충돌을 거부한다. 알려진 secret/data/developer 경로와 파일 확장자를 거부하지만 일반적인 모든 내용 유출을 탐지하는 DLP는 아니다. source·키·DB가 없는 staging은 별도 검토한다.

## 한도와 출력

| 항목 | 한도 |
| --- | --- |
| gzip archive | 1 GiB |
| payload 합계 | 4 GiB |
| 개별 payload | 512 MiB |
| 파일 수 | 10,000 |
| manifest | 1 MiB |
| JSON nesting | 32 |
| key PEM | 16 KiB |

Verify 결과는 `manifest`, `archiveSha256`, `manifestSha256`, `authenticity`, `filesVerified`, `bytesVerified`다. authenticity는 `verified-external-ed25519` 또는 `unsigned-development`다. 어떤 host에서 검사해도 **내용·서명 검증**이며 실행 host compatibility를 확인한 것은 아니다.

v1 platform은 `darwin/arm64/none/none` 또는 기존 `linux/amd64/glibc/2.34` 조합이다(OS/arch/libc/minGlibc 순서). Java 21·RocksDB는 유지한다. Mac의 none은 glibc 조건 비적용을 뜻한다. 기존 Linux archive를 Rust에서 읽을 수 있도록 형식을 유지했으며 예전 Go verifier는 신규 Mac profile을 거부하는 것이 정상이다. Rust/Go 압축 구현 사이에 archive bytes가 같을 필요는 없고 각각 같은 고정 입력에서 반복 조립 결과가 같아야 한다.

고정 입력·고정 key의 archive는 파일 정렬과 tar/gzip metadata 정규화로 재현 가능하게 만든다. CLI 바이너리·JRE·JAR 자체의 재현 빌드와는 별도 주장이다. 원본 input은 빌드 중 다른 작업이 수정하지 않는 전용 stage를 사용한다.

## 공식 엔진 공급 이후

NIGO artifact/manifest·JRE 공급자가 정해지면 실제 `engine.lock.json`과 `packaging/runtime.lock.json`을 공급 provenance에 연결한다. 현재는 가짜 release/tag/hash의 lock을 만들지 않았다. 호환된 공식 contract status·runtime provenance·실제 Linux native 인수를 추가한 별도 버전에서 고객 후보 채널을 열어야 한다.
